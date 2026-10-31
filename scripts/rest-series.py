#!/usr/bin/env python3
"""Submit a complete series for ONE local approval; never approves or retries it.
Usage: python3 scripts/rest-series.py examples/series-request.json
       python3 scripts/rest-series.py --status SERIES_ID
"""
import getpass
import json
import sys
import urllib.request

if len(sys.argv) not in (2, 3):
    raise SystemExit(__doc__)
if sys.argv[1] == '--status' and len(sys.argv) == 3:
    op, args = 'series.get', {'id': sys.argv[2]}
else:
    with open(sys.argv[1], encoding='utf-8') as stream:
        args = json.load(stream)
    op = 'series.submit'
token = getpass.getpass('FrameStudio REST-Zugriffstoken: ')
request = urllib.request.Request('http://127.0.0.1:43120/v1/operations/' + op,
    data=json.dumps(args).encode(), headers={'Authorization': 'Bearer ' + token,
    'Content-Type': 'application/json'}, method='POST')
with urllib.request.urlopen(request, timeout=120) as response:
    result = json.load(response)
print(json.dumps({key: result.get(key) for key in (
    'id', 'description', 'origin', 'status', 'count', 'estimated_microusd',
    'currency', 'job_ids', 'job_status_counts')}, indent=2, ensure_ascii=False))
if result['status'] == 'pending':
    print('In FrameStudio unter Serienfreigaben einmalig prüfen und freigeben.')
