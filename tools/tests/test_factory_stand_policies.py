"""Stand policy setup uses the public versioned management contract."""
import copy
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
import uuid

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import factory_triage_setup as triage
import factory_delivery_setup as delivery


class PolicyManagement:
    """Finite external Server adapter; reads expose only published policy versions."""
    def __init__(self, project, permissions):
        self.project = project
        self.records = {}
        self.replays = {}
        self.launch = self.publish('launch', str(uuid.uuid4()), {
            'name': 'Standard', 'allow_manual': True, 'allow_automatic_source': True})
        self.access = self.publish('access', str(uuid.uuid4()), {'name': 'Standard', 'permissions': permissions})
        self.defaults = {'project_id': project, 'version': 1,
            'launch_policy': self.launch['policy']['reference'], 'access_profile': self.access['policy']['reference']}

    def publish(self, kind, identity, definition):
        versions = self.records.setdefault((kind, identity), [])
        reference = {'project_id': self.project, 'id': identity, 'version': len(versions) + 1,
            'digest': hashlib.sha256(json.dumps(definition, sort_keys=True).encode()).hexdigest()}
        record = {'kind': kind, 'policy': {'reference': reference, 'definition': copy.deepcopy(definition)}}
        versions.append(record)
        return copy.deepcopy(record)

    def get(self, path):
        if path.endswith('/defaults'):
            return {'defaults': copy.deepcopy(self.defaults)}
        if '/versions/' in path:
            parts = path.split('?')[0].split('/')
            return {'policy': copy.deepcopy(self.records[(parts[-4], parts[-3])][int(parts[-1]) - 1])}
        return {'items': [{'name': records[-1]['policy']['definition']['name'],
            'reference': {'kind': kind, 'reference': records[-1]['policy']['reference']}}
            for (kind, _), records in self.records.items()], 'next_cursor': None}

    def post(self, path, key, body):
        replay = (path, key)
        if replay in self.replays:
            previous, result = self.replays[replay]
            assert previous == body, 'Idempotency key reused with different content'
            return copy.deepcopy(result)
        if path.endswith('/provision'):
            result = {'created': True, 'disposition': 'applied', 'unresolved': [],
                'provisioning': {'standard_access': self.access['policy']['reference']},
                'defaults': copy.deepcopy(self.defaults)}
        elif path.endswith('/defaults'):
            assert body['expected_version'] == self.defaults['version']
            self.defaults = {'project_id': self.project, 'version': self.defaults['version'] + 1,
                'launch_policy': body['launch_policy'], 'access_profile': body['access_profile']}
            result = {'defaults': copy.deepcopy(self.defaults)}
        elif path.endswith('/validate'):
            result = {'policy': {'kind': body['kind'], 'policy': {'definition': body['definition']}}}
        else:
            versions = self.records.get((body['kind'], body['id']), [])
            assert body['expected_current'] == (versions[-1]['policy']['reference'] if versions else None)
            result = {'disposition': 'applied',
                'policy': self.publish(body['kind'], body['id'], body['definition'])}
        self.replays[replay] = copy.deepcopy((body, result))
        return copy.deepcopy(result)


@unittest.skipUnless(os.name == 'posix', 'private stand catalogs require POSIX ownership')
class FactoryStandPoliciesTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.catalog = Path(temporary.name) / 'catalog.json'
        self.project, self.namespace = str(uuid.uuid4()), uuid.uuid4()
        self.permissions = {'mounts': [{'root': '/octacity/protected', 'mode': 'read_only'}],
            'secret_profiles': ['triage-model', 'triage-mcp']}
        self.installed = {'projects': [{'project_id': self.project, 'references': [{
            'kind': 'permission_ceiling', 'alias': 'triage-ceiling',
            'reference': {'identity': 'triage-ceiling', 'version': 'v1', 'digest': 'a' * 64}}]}]}
        self.catalog.write_text(json.dumps(self.installed))
        self.catalog.chmod(0o600)
        self.client = PolicyManagement(self.project, self.permissions)
        self.base = f'/api/v1/projects/{self.project}/factory-policies'

    def test_provisioning_names_triage_access_and_preserves_the_standard_launch(self):
        launch = copy.deepcopy(self.client.launch)
        original_access = copy.deepcopy(self.client.access)
        receipt = triage.provision_project_policies(self.client, self.project, self.namespace, self.catalog)
        policies = self.client.get(self.base)['items']
        self.assertEqual({(x['reference']['kind'], x['name']) for x in policies},
            {('launch', 'Standard'), ('access', 'Triage')})
        access = next(x['reference']['reference'] for x in policies if x['reference']['kind'] == 'access')
        self.assertEqual(access['id'], original_access['policy']['reference']['id'])
        record = self.client.get(f"{self.base}/access/{access['id']}/versions/{access['version']}")['policy']
        self.assertEqual(record['policy']['definition']['permissions'], self.permissions)
        self.assertEqual(receipt['defaults']['access_profile'], access)
        self.assertEqual(self.client.get(self.base + '/defaults')['defaults']['launch_policy'], launch['policy']['reference'])
        self.assertEqual(self.client.get(f"{self.base}/access/{access['id']}/versions/1")['policy'], original_access)

    def test_provisioning_does_not_select_an_operator_modified_triage_version(self):
        original_defaults = copy.deepcopy(self.client.defaults)
        self.client.publish('access', self.client.access['policy']['reference']['id'], {
            'name': 'Triage', 'permissions': {'secret_profiles': ['operator-credential']}})
        triage.provision_project_policies(self.client, self.project, self.namespace, self.catalog)
        self.assertEqual(self.client.get(self.base + '/defaults')['defaults'], original_defaults)

    def test_delivery_adopts_existing_equivalent_permissions_without_republishing(self):
        plugins = [{'identity': name, 'version': 'v1', 'digest': digit * 64}
            for name, digit in [('codex', '1'), ('shell', '2')]]
        self.client.publish('access', str(uuid.uuid4()), {'name': 'Delivery', 'permissions': {'plugins': plugins}})
        self.installed['projects'][0]['references'].append({'kind': 'permission_ceiling',
            'alias': 'delivery-ceiling-reviewed', 'reference': triage._reference('delivery-ceiling-reviewed'),
            'metadata': {'permission_declarations': {'plugins': [dict(reversed(list(p.items())))
                for p in reversed(plugins)]}}})
        self.catalog.write_text(json.dumps(self.installed))
        before, defaults = self.client.get(self.base), self.client.get(self.base + '/defaults')
        delivery.provision_access_profile(self.client, self.project, self.namespace, self.catalog)
        self.assertEqual(self.client.get(self.base), before)
        self.assertEqual(self.client.get(self.base + '/defaults'), defaults)
