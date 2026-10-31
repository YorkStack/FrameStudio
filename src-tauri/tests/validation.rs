use framestudio::core::Core;
use serde_json::json;
#[tokio::test]
async fn malformed_generation_types_are_not_silently_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let core = Core::open(dir.path().into()).unwrap();
    let project = core
        .call("project.save", json!({"name":"Type guards"}), false)
        .await
        .unwrap();
    let request = json!({"project_id":project["id"],"description":"synthetic test","provider":"test","model":"deterministic-v1","aspect_ratio":"1:1","resolution":"1K","count":1});
    for field in ["count", "provider", "model", "aspect_ratio", "resolution"] {
        let mut invalid = request.clone();
        invalid[field] = json!(-2);
        assert!(
            core.call("cost.estimate", invalid, false).await.is_err(),
            "{field}"
        );
    }
    let mut invalid = request;
    invalid["count"] = json!("3");
    assert!(core.call("cost.estimate", invalid, false).await.is_err());
}
