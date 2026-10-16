#!/usr/bin/env python3
"""Hermetic Source process peer. No network or credential resolution."""
import fcntl
import json
import sys
import time

def event(kind, request_id):
    with open('events.jsonl', 'a', encoding='utf-8') as events:
        fcntl.flock(events, fcntl.LOCK_EX)
        events.write(json.dumps({'event': kind, 'request_id': request_id}) + '\n')

request = json.loads(sys.stdin.readline())
event('start', request['message']['request_id'])
with open('requests.jsonl', 'a', encoding='utf-8') as calls:
    fcntl.flock(calls, fcntl.LOCK_EX)
    calls.write(json.dumps(request) + '\n')
mode = json.load(open('mode.json', encoding='utf-8'))
intent = request['message']['command']['payload']
if request['message']['command']['operation'] == 'scan' and mode.get('hold'):
    with open('ready', 'w', encoding='utf-8') as ready:
        ready.write('ready')
    cancel = sys.stdin.readline()
    with open('cancelled', 'w', encoding='utf-8') as stopped:
        stopped.write(cancel)
    sys.exit(0)
if request['message']['command']['operation'] == 'authorize':
    if mode.get('deny_access'):
        outcome = {'status': 'failure', 'payload': {'class': 'permanent', 'retry_after_millis': None}}
    else:
        outcome = {'status': 'authorized', 'payload': {'scan_id': intent['scan_id'], 'resource_id': intent['resource']['resource_id'], 'configuration_digest': intent['configuration_digest']}}
elif 'failure' in mode:
    outcome = {'status': 'failure', 'payload': mode['failure']}
else:
    time.sleep(mode.get('delay', 0))
    candidates = mode.get('candidates', [])
    payload = {key: intent[key] for key in ('scan_id', 'configuration_digest', 'selection')}
    payload.update(resource_id=intent['resource']['resource_id'], candidates=candidates, next_cursor=None)
    if intent.get('additional_selections'):
        selections = [intent['selection']] + intent['additional_selections']
        payload['matching_selections'] = [[selections[i] for i in candidate.pop('matches', [0])] for candidate in candidates]
    outcome = {'status': 'page', 'payload': payload}
event('finish', request['message']['request_id'])
json.dump({'family': 'source', 'message': {'protocol_version': 1, 'request_id': request['message']['request_id'], 'outcome': outcome}}, sys.stdout)
sys.stdout.write('\n')
