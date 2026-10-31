#!/usr/bin/env python3
"""Run with Python 3. Token is prompted without echo, never embedded or logged."""
import getpass
import json
import urllib.request

token = getpass.getpass('FrameStudio local API token: ')

def call(op, args):
    request = urllib.request.Request('http://127.0.0.1:43120/v1/operations/' + op,
        data=json.dumps(args).encode(), headers={'Authorization': 'Bearer ' + token,
        'Content-Type': 'application/json'}, method='POST')
    with urllib.request.urlopen(request, timeout=120) as response:
        return json.load(response)

print(json.dumps(call('project.list', {}), indent=2, ensure_ascii=False))
