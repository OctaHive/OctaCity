#!/usr/bin/env python3
"""Hermetic Source process peer. No network or credential resolution."""
import fcntl
import json
import hashlib
import os
import sys
import time

def event(kind, request_id):
    with open('events.jsonl', 'a', encoding='utf-8') as events:
        fcntl.flock(events, fcntl.LOCK_EX)
        events.write(json.dumps({'event': kind, 'request_id': request_id}) + '\n')

request = json.loads(sys.stdin.readline())
if request['family'] == 'reporter':
    with open('reporter-requests.jsonl', 'a', encoding='utf-8') as calls:
        fcntl.flock(calls, fcntl.LOCK_EX)
        calls.write(json.dumps(request) + '\n')
    command = request['message']['command']['operation']
    payload = request['message']['command']['payload']
    intent = payload['intent'] if command == 'apply' else payload
    mode = json.load(open('reporter-mode.json', encoding='utf-8')) if os.path.exists('reporter-mode.json') else {}
    if 'failure' in mode:
        json.dump({'family':'reporter', 'message':{'protocol_version':request['message']['protocol_version'], 'request_id':request['message']['request_id'],
            'outcome':{'status':'failure', 'payload':mode['failure']}}}, sys.stdout)
        sys.stdout.write('\n')
        sys.exit(0)
    time.sleep(mode.get('delay', 0))
    if command in ('observe', 'inspect') and mode.get('pause_observe'):
        with open('reporter-observing', 'w', encoding='utf-8') as ready:
            ready.write(intent['projection_id'])
        while not os.path.exists('reporter-release'):
            time.sleep(0.005)
    if command in ('observe', 'inspect') and mode.get('lose_observation_response'):
        sys.stdout.write('{"family":"reporter"')
        sys.exit(0)
    if command in ('inspect', 'apply'):
        fields = mode.get('fieldsets', {}).get(intent['mapping']['identity'], ['a' * 64])
        desired = intent['mapping']['identity']
        def read_state():
            with open('reporter-state.json', 'a+', encoding='utf-8') as state:
                fcntl.flock(state, fcntl.LOCK_SH)
                state.seek(0)
                text = state.read()
                return json.loads(text) if text else {}
        def plan(values):
            receipt = {key: intent[key] for key in ('projection_id', 'item_id', 'configuration_digest', 'mapping', 'status')}
            receipt['resource_id'] = intent['resource']['resource_id']
            receipt['matches'] = all(values.get(field) == desired for field in fields)
            token = hashlib.sha256(json.dumps([intent, {field: values.get(field) for field in fields}], sort_keys=True).encode()).hexdigest()
            return {'receipt':receipt, 'fields':fields, 'steps':[] if receipt['matches'] else [{'fields':fields, 'token':token}]}
        current = plan(read_state())
        if command == 'apply':
            if mode.get('conflict_on_apply') or payload['step'] not in current['steps']:
                json.dump({'family':'reporter', 'message':{'protocol_version':2, 'request_id':request['message']['request_id'],
                    'outcome':{'status':'failure', 'payload':{'class':'conflict','retry_after_millis':None}}}}, sys.stdout)
                sys.stdout.write('\n')
                sys.exit(0)
            if mode.get('pause_before_apply'):
                with open('reporter-before-apply', 'w', encoding='utf-8') as ready:
                    ready.write(intent['projection_id'])
                while not os.path.exists('reporter-release-before'):
                    time.sleep(0.005)
            with open('reporter-state.json', 'a+', encoding='utf-8') as state:
                fcntl.flock(state, fcntl.LOCK_EX)
                state.seek(0)
                text = state.read()
                values = json.loads(text) if text else {}
                for field in fields:
                    values[field] = 'human-edit' if mode.get('undo_after_apply') else desired
                state.seek(0)
                state.truncate()
                json.dump(values, state)
            with open('reporter-applied', 'w', encoding='utf-8') as applied:
                applied.write(intent['projection_id'])
            if mode.get('pause_after_apply'):
                with open('reporter-awaiting-response', 'w', encoding='utf-8') as ready:
                    ready.write(intent['projection_id'])
                while not os.path.exists('reporter-release'):
                    time.sleep(0.005)
            if mode.get('lose_project_response'):
                sys.stdout.write('{"family":"reporter"')
                sys.exit(0)
        json.dump({'family':'reporter', 'message':{'protocol_version':2, 'request_id':request['message']['request_id'],
            'outcome':{'status':'progressed' if command == 'apply' else 'planned','payload':plan(read_state())}}}, sys.stdout)
        sys.stdout.write('\n')
        sys.exit(0)
    receipt = {key: intent[key] for key in ('projection_id', 'item_id', 'configuration_digest', 'mapping', 'status')}
    receipt['resource_id'] = intent['resource']['resource_id']
    receipt['matches'] = os.path.exists('reporter-applied')
    if command == 'project':
        with open('reporter-applied', 'w', encoding='utf-8') as applied:
            applied.write(intent['projection_id'])
        if mode.get('pause_after_apply'):
            with open('reporter-awaiting-response', 'w', encoding='utf-8') as ready:
                ready.write(intent['projection_id'])
            while not os.path.exists('reporter-release'):
                time.sleep(0.005)
        if mode.get('lose_project_response'):
            sys.stdout.write('{"family":"reporter"')
            sys.exit(0)
        receipt['matches'] = True
    json.dump({'family':'reporter', 'message':{'protocol_version':1, 'request_id':request['message']['request_id'],
        'outcome':{'status':'applied' if command == 'project' else 'observed', 'payload':receipt}}}, sys.stdout)
    sys.stdout.write('\n')
    sys.exit(0)
event('start', request['message']['request_id'])
with open('requests.jsonl', 'a', encoding='utf-8') as calls:
    fcntl.flock(calls, fcntl.LOCK_EX)
    calls.write(json.dumps(request) + '\n')
mode = json.load(open('mode.json', encoding='utf-8'))
intent = request['message']['command']['payload']
if request['message']['command']['operation'] == 'scan' and mode.get('pause_scan'):
    with open('ready', 'w', encoding='utf-8') as ready:
        ready.write('ready')
    while not os.path.exists('release'):
        time.sleep(0.005)
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
