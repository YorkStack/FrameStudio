use super::*;
pub(super) fn active_projects(conn: &Connection) -> Result<Vec<Value>> {
    Ok(list(conn, "project", None)?
        .into_iter()
        .filter(|p| p["deleted_at"].is_null())
        .collect())
}
pub(super) fn validate_template(value: &Value) -> Result<()> {
    validate_named_blocks(value, &["scene", "character", "instruction"])
}
pub(super) fn save_prompt(conn: &Connection, args: &Value) -> Result<Value> {
    let mut project = get(conn, "project", required(args, "id")?)?;
    ensure!(
        project["deleted_at"].is_null(),
        "Projekt liegt im Papierkorb"
    );
    validate_template(&args["prompt_template"])?;
    project["prompt_template"] = args["prompt_template"].clone();
    project["updated_at"] = json!(now());
    put(conn, "project", &project)?;
    Ok(project)
}
pub(super) fn trash(conn: &mut Connection, op: &str, args: &Value) -> Result<Value> {
    if op == "project.trash.list" {
        return Ok(json!(
            list(conn, "project", None)?
                .into_iter()
                .filter(|p| !p["deleted_at"].is_null())
                .collect::<Vec<_>>()
        ));
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut p = get(&tx, "project", required(args, "id")?)?;
    if op == "project.trash" {
        ensure!(
            !job_history(&tx, text(&p, "id"))?
                .iter()
                .any(|j| ["queued", "running", "cancelling"].contains(&text(j, "status"))),
            "Projekt hat offene Aufträge. Bitte zuerst abschließen oder abbrechen."
        );
        ensure!(
            !list(&tx, "series", Some(text(&p, "id")))?
                .iter()
                .any(|s| s["status"] == "pending"),
            "Projekt hat eine offene Serienfreigabe. Bitte zuerst ablehnen."
        );
        p["deleted_at"] = json!(now());
    } else {
        p["deleted_at"] = Value::Null;
    }
    p["updated_at"] = json!(now());
    put(&tx, "project", &p)?;
    tx.commit()?;
    Ok(p)
}

pub(super) fn characters(conn: &Connection, project: Option<&str>) -> Result<Vec<Value>> {
    let mut chars = list(conn, "character", project)?;
    for c in &mut chars {
        if c["portrait_asset_id"]
            .as_str()
            .is_none_or(|id| id.is_empty())
        {
            let id: Option<String> = conn.query_row("SELECT id FROM records WHERE kind='asset' AND project_id=?1 AND json_extract(data,'$.snapshot.character.id')=?2 AND json_extract(data,'$.provider')!='import' ORDER BY created_at ASC LIMIT 1",params![text(c,"project_id"),text(c,"id")],|r|r.get(0)).optional()?;
            if let Some(id) = id {
                c["portrait_asset_id"] = json!(id);
                c["portrait_automatic"] = json!(true);
            }
        }
    }
    Ok(chars)
}
