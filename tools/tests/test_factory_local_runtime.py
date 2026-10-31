"""Local Runtime installation preserves provider metadata and isolates login files."""
import copy
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import sys
import tarfile
import tempfile
import types
import unittest
from unittest.mock import patch
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

@unittest.skipUnless(os.name == "posix", "private Runtime installation requires POSIX ownership")
class FactoryLocalRuntimeTests(unittest.TestCase):
    def test_runtime_companions_are_pinned_repeatable_and_cannot_replace_retained_bytes(self):
        import factory_local_runtime as runtime
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            bundle = base / "runtime"
            bundle.mkdir(mode=0o700)
            payload = b"qualified runtime companion"
            archive = base / "companion.tar.gz"
            with tarfile.open(archive, "w:gz") as output:
                member = tarfile.TarInfo("helper-linux"); member.size = len(payload)
                output.addfile(member, io.BytesIO(payload))
            policy = base / "companions.json"
            policy.write_text(json.dumps({"runtime_version":"0.161.0", "executables":[{
                "identity":"runtime-helper", "filename":"runtime-helper", "platform":"linux-aarch64",
                "archive":{"version":"0.161.0", "executable":"helper-linux",
                    "sha256":hashlib.sha256(archive.read_bytes()).hexdigest(),
                    "expanded_max_bytes":4096, "file_max_bytes":4096, "member_max_count":4}}]}))
            selected = {"bundle":bundle,"policy":{"archive":{"version":"0.161.0"}}}
            with patch.object(runtime, "COMPANION_POLICY", policy), patch.object(runtime, "download", side_effect=lambda _, target: shutil.copyfile(archive,target)) as fetched:
                companions = runtime.install_runtime_companions(selected)
                self.assertEqual(len(companions), 1)
                self.assertEqual(companions[0]["identity"], "runtime-helper")
                self.assertEqual(companions[0]["guest_filename"], "runtime-helper")
                self.assertEqual(companions[0]["sha256"], hashlib.sha256(payload).hexdigest())
                installed = Path(companions[0]["path"])
                self.assertEqual(installed.read_bytes(), payload)
                self.assertEqual(installed.stat().st_mode & 0o777, 0o700)
                self.assertEqual(runtime.install_runtime_companions(selected), companions)
                self.assertEqual(fetched.call_count, 1, "repeat startup must reuse the verified installation offline")
                installed.write_bytes(b"substituted companion")
                with self.assertRaisesRegex(ValueError, "companion"):
                    runtime.install_runtime_companions(selected)
                self.assertEqual(fetched.call_count, 1, "drift requires repair, not silent replacement")

    def test_native_login_installation_seeds_only_auth_once_and_preserves_native_updates(self):
        import factory_local_runtime as runtime
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary).resolve()
            home = base / "operator"
            home.mkdir(mode=0o700)
            seed = {"auth_mode": "chatgpt", "tokens": {"access_token": "seed-private-canary", "refresh_token": "seed-refresh-canary"}}
            source = home / "auth.json"
            source.write_text(json.dumps(seed))
            source.chmod(0o600)
            (home / "config.toml").write_text("private MCP and local instructions")
            root = base / "stand"
            reference = runtime.install_native_login(root, home=home)
            target = Path(reference["path"])
            self.assertTrue(target.is_relative_to(root / "secrets"))
            self.assertEqual(reference["destination"], "codex/auth.json")
            self.assertEqual(reference["sensitive_fields"], ["/tokens"])
            self.assertIs(reference["persist_updates"], True)
            self.assertEqual(json.loads(target.read_text()), seed)
            self.assertEqual(target.stat().st_mode & 0o777, 0o600)
            self.assertEqual(target.parent.stat().st_mode & 0o777, 0o700)
            self.assertEqual((root / "secrets").stat().st_mode & 0o777, 0o700)
            self.assertEqual([path.name for path in target.parent.iterdir()], ["auth.json"])
            self.assertNotIn("private-canary", json.dumps(reference))
            updated = copy.deepcopy(seed)
            updated["tokens"]["access_token"] = "refreshed-private-canary"
            target.write_text(json.dumps(updated))
            again = runtime.install_native_login(root, home=home)
            self.assertEqual(again, reference)
            self.assertEqual(json.loads(target.read_text()), updated)
            self.assertEqual(json.loads(source.read_text()), seed)

    def test_native_subscription_login_rejects_an_api_key_cache_without_creating_the_stand(self):
        import factory_local_runtime as runtime
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary).resolve()
            home = base / "operator"
            home.mkdir(mode=0o700)
            source = home / "auth.json"
            source.write_text('{"auth_mode":"apikey","OPENAI_API_KEY":"private-api-canary"}')
            source.chmod(0o600)
            root = base / "stand"
            with self.assertRaises(ValueError) as rejected:
                runtime.install_native_login(root, home=home)
            self.assertNotIn("private-api-canary", str(rejected.exception))
            self.assertFalse(root.exists())

    def test_installs_a_qualified_runtime_and_only_authentication_without_model_name_defaults(self):
        import factory_local_runtime as runtime
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            source = base / 'runtime.tar.gz'
            with tarfile.open(source, 'w:gz') as archive:
                content = b'#!/bin/sh\necho codex-cli 0.161.0\n'
                member = tarfile.TarInfo('codex-test'); member.size = len(content); member.mode = 0o755
                archive.addfile(member, io.BytesIO(content))
                helper = b"qualified native companion"
                member = tarfile.TarInfo("helper-test"); member.size = len(helper); member.mode = 0o755
                archive.addfile(member, io.BytesIO(helper))
            policy = json.loads(runtime.POLICY.read_text())
            policy['archive'].update(sha256=hashlib.sha256(source.read_bytes()).hexdigest(), executable='codex-test')
            policy_path = base / 'runtime.json'; policy_path.write_text(json.dumps(policy))
            companions = json.loads(runtime.COMPANION_POLICY.read_text())
            companions['executables'][0]['archive'].update(sha256=hashlib.sha256(source.read_bytes()).hexdigest(), executable='helper-test')
            companion_policy = base / 'companions.json'; companion_policy.write_text(json.dumps(companions))
            native = base / 'native/oct a'; native.mkdir(parents=True)
            (native / 'codex-compatibility.json').write_text(json.dumps({'plugin':{'capabilities':['codex.task-runtime-settings.v1']},'executable':{'supported_versions':['0.161.0'], 'selection_environment':'OCTA_CODEX_EXECUTABLE'}}))
            (native.parent / 'job-spec-policy.json').write_text(json.dumps({'octa':{'version':'0.7.2', 'plugin_digests':{'codex':'a'*64}}}))
            home = base / 'operator'; home.mkdir(mode=0o700)
            (home / 'auth.json').write_text('{"auth_mode":"chatgpt","tokens":{"private":"login-canary"}}'); (home / 'auth.json').chmod(0o600)
            (home / 'config.toml').write_text('private MCP and local instructions')
            api = types.SimpleNamespace(get=lambda _: {'id':'cfg','project_id':'project','version':1,'definition':{'enabled':True}},
                post=lambda *args: {'reference':{'identity':'runtime-parameters','version':'v1','digest':'b'*64},'shape':{'type':'object','fields':{}}})
            with patch.object(runtime, 'POLICY', policy_path), patch.object(runtime, 'COMPANION_POLICY', companion_policy), patch.object(runtime, 'download', side_effect=lambda _, target: shutil.copyfile(source,target)), \
                    patch.object(runtime.subprocess, 'run', return_value=types.SimpleNamespace(stdout='c'*64+'  plugin\n')):
                installed = runtime.install_runtime(base/'stand', native.parent, {'project':'project','configuration':'cfg'}, api, home=home, octa_root=native)
                legacy = copy.deepcopy(installed['project'])
                legacy['role_profiles']['codex'].pop('settings_support', None)
                cached = installed['bundle']/'project.json'
                cached.write_text(json.dumps(legacy))
                retained_catalog = cached.read_bytes()
                again = runtime.install_runtime(base/'stand', native.parent, {'project':'project','configuration':'cfg'}, None, home=base/'no-login', octa_root=native)
                self.assertEqual(again['project'],installed['project'])
                self.assertEqual(cached.read_bytes(),retained_catalog)
                self.assertEqual(again['project']['role_profiles']['codex']['settings_support'], {'mcp_transports':['http','stdio'],'mcp_executables':{}})
                anonymous = runtime.install_runtime(base/'anonymous', native.parent, {'project':'project','configuration':'cfg'}, api, home=base/'no-login', octa_root=native)
                self.assertFalse((anonymous['bundle']/'home/.codex/auth.json').exists())
                with patch.dict(os.environ, {'CODEX_HOME': str(home)}):
                    agent_only = runtime.install_runtime_executable(base/'agent-only', native.parent, octa_root=native)
                self.assertEqual(agent_only['executable'].read_bytes(), content)
                self.assertEqual(agent_only['sha256'], hashlib.sha256(content).hexdigest())
                self.assertEqual(Path(agent_only['companions'][0]['path']).read_bytes(), helper)
                self.assertEqual(agent_only['companions'][0]['guest_filename'], 'codex-code-mode-host')
                self.assertFalse((agent_only['bundle']/'home').exists(), 'binary installation cannot import host authentication or configuration')
            bundle = installed['bundle']
            self.assertEqual((bundle/'codex').read_bytes(), content)
            self.assertEqual((bundle/'home/.codex/auth.json').read_text(), (home/'auth.json').read_text())
            self.assertFalse((bundle/'home/.codex/config.toml').exists())
            self.assertEqual((bundle/'home/.codex/auth.json').stat().st_mode & 0o777,0o600)
            declaration = installed['project']['role_profiles']['codex']
            self.assertEqual(declaration['model_discovery']['environment']['HOME'],'/run/octacity-runtime/home')
            self.assertEqual(declaration['model_discovery']['environment']['OCTA_CODEX_EXECUTABLE'],'/run/octacity-runtime/codex')
            self.assertNotIn('login-canary',json.dumps(installed['project']))
            self.assertNotIn('models',declaration)
            self.assertEqual(declaration['ceiling']['model_parameters'],{})
            self.assertEqual(declaration['ceiling']['build_configuration']['id'],'cfg')
            (bundle/'codex').write_text('changed runtime bytes')
            with patch.object(runtime, 'POLICY', policy_path), patch.object(runtime.subprocess, 'run', return_value=types.SimpleNamespace(stdout='c'*64+'  plugin\n')):
                with self.assertRaisesRegex(ValueError, 'runtime identity'):
                    runtime.install_runtime(base/'stand', native.parent, {'project':'project','configuration':'cfg'}, api, home=home, octa_root=native)


if __name__ == '__main__': unittest.main()

@unittest.skipUnless(__import__('os').environ.get('OCTACITY_TEST_FACTORY_RUNTIME') == '1', 'requires the qualified Codex archive, login snapshot and local Docker images')
class FactoryRuntimeRestTests(unittest.TestCase):
    def test_real_plugin_catalog_publishes_independent_default_and_pro_without_starting_builds(self):
        import contextlib
        import os
        import subprocess
        import uuid
        import bootstrap_local_stand as bootstrap
        import configure_local_stand_server as configure
        import factory_local_stand as stand
        from tools.tests import factory_demo_support as demo_support
        import factory_local_runtime as runtime
        import local_stand_launcher as launcher
        import init_local_stand as initializer
        image = os.environ['OCTACITY_TEST_SERVER_IMAGE']
        archive = Path(os.environ['OCTACITY_TEST_CODEX_ARCHIVE'])
        release = Path(os.environ['OCTACITY_TEST_OCTA_ROOT'])
        if subprocess.run(['docker','ps','--all','--filter','label=com.docker.compose.project=octacity-local','--format','{{.ID}}'],capture_output=True,text=True,check=True).stdout.strip():
            self.skipTest('the personal local stand is in use')
        with tempfile.TemporaryDirectory(prefix='octacity-runtime-rest-') as temporary:
            root = Path(temporary)/'stand'
            initializer.initialize(root)
            native = Path(temporary)/'native'; native.mkdir(mode=0o700)
            (native/'octa').mkdir(mode=0o700)
            shutil.copyfile(release/'codex-compatibility.json',native/'octa/codex-compatibility.json')
            # Startup-only JobSpec policy fixture: this journey admits no Job or Build.
            plugin_digest = next(line.split(': ',1)[1] for line in (release/'plugins/codex.plugin.yml').read_text().splitlines() if line.startswith('sha256:'))
            policy = {'source':{'provider':'git','plugin_version':'0.1.0','plugin_sha256':'1'*64,'repository_parameter':'url'},
                'octa':{'version':'0.7.2','runner_sha256':'2'*64,'runner_protocol':1,'event_schema':1,'plugin_protocol':1,'plugin_digests':{'codex':plugin_digest}},'validity':900}
            payload = json.dumps(policy).encode()
            (native/'job-spec-policy.json').write_bytes(payload); (native/'job-spec-policy.json').chmod(0o600)
            (native/'installation-manifest.json').write_text(json.dumps({'schema_version':1,'files':{'job-spec-policy.json':hashlib.sha256(payload).hexdigest()}})); (native/'installation-manifest.json').chmod(0o600)
            configure.generate_server_configuration(root,native)
            server = None
            try:
                launcher._run_compose(root,8443,('up','--detach','--wait','postgres','minio'),timeout_seconds=180)
                launcher._run_compose(root,8443,('run','--rm','--no-deps','minio-init'),timeout_seconds=60)
                launcher._run_compose(root,8443,('run','--rm','--no-deps','--entrypoint','/bin/true','gateway'),timeout_seconds=30)
                namespace = str(uuid.uuid4())
                with stand.bootstrap_server(root,native) as client:
                    pool = client.post(bootstrap.POOL_PATH,namespace+'-pool',bootstrap.LOCAL_POOL_REQUEST)['resource']['id']
                    resources = demo_support.prepare_resources(client,pool,'https://github.com/OctaHive/OctaCity',stand.integration._guest_image(),namespace)
                    pinned_download = runtime.download
                    primary_digest = json.loads(runtime.POLICY.read_text())['archive']['sha256']
                    def download_asset(policy, target):
                        if policy['sha256'] == primary_digest:
                            shutil.copyfile(archive, target)
                        else:
                            pinned_download(policy, target)
                    with patch.object(runtime,'SERVER_IMAGE',image), patch.object(runtime,'download',side_effect=download_asset):
                        installed = runtime.install_runtime(root,native,resources,client)
                    profiles = root/'profiles.json'
                    stand._private_json(profiles,{'version':1,'projects':[installed['project']],'codex':[]})
                    # Keep the existing bootstrap gateway alive for the object-store TLS origin.
                    (root/'config/server').rename(root/'config/bootstrap-server')
                    receipt = configure.generate_server_configuration(root,native,factory_profiles=profiles,factory_runtime_bundle=installed['bundle'])
                    mounts=[]
                    for source,destination in [(Path(receipt['config']),'/run/octacity/server.toml'),(Path(receipt['job_spec_policy']),'/run/octacity/job-spec-policy.json'),(root/'config/server','/run/octacity-factory-connectors'),(root/'pki/local-ca.pem','/etc/ssl/certs/octacity-local-ca.pem')]:
                        mounts += ['--mount',f'type=bind,src={source},dst={destination},readonly']
                    for name in ('object-access-key','object-secret-key','signing-key','agent-enrollment-key','cache-credential-key'):
                        mounts += ['--mount',f'type=bind,src={root/"secrets"/name},dst=/run/secrets/{name},readonly']
                    mounts += ['--mount',f'type=bind,src={receipt["postgres_url"]},dst=/run/secrets/postgres-url,readonly',
                        '--mount',f'type=bind,src={profiles},dst=/run/secrets/factory-profiles.json,readonly']
                    server=subprocess.run(['docker','run','--detach','--network','octacity-local_stand','--network','octacity-local_edge','--user','0:0',
                        '--publish','127.0.0.1::8080','--read-only','--tmpfs','/run/octacity-secrets:mode=0700,size=320m',
                        '--tmpfs','/run/octacity-runtime:exec,mode=0700,size=512m',*mounts,image,'run','/run/octacity/server.toml'],check=True,capture_output=True,text=True).stdout.strip()
                    port=subprocess.run(['docker','port',server,'8080/tcp'],check=True,capture_output=True,text=True).stdout.strip().rsplit(':',1)[1]
                    api=bootstrap.BootstrapClient(stand.BootstrapTransport(int(port)),bootstrap.RetryPolicy(attempts=1,timeout_seconds=3,initial_delay_seconds=0.25))
                    import time
                    deadline=time.monotonic()+40
                    while True:
                        try: api.ready(); break
                        except (OSError,bootstrap.BootstrapError):
                            if time.monotonic()>deadline:
                                self.fail('Runtime Server did not become ready: '+subprocess.run(['docker','logs',server],capture_output=True,text=True).stderr[-1500:])
                            time.sleep(0.25)
                    base=f"/api/v1/projects/{resources['project']}"
                    deadline=time.monotonic()+30
                    while True:
                        response=api._request('GET',base+'/factory-profile-capabilities?limit=25',{},None)
                        if response.status==200:
                            choices=json.loads(response.body); break
                        if time.monotonic()>deadline:
                            self.fail('Factory catalog did not become ready: '+response.body.decode())
                        time.sleep(0.25)
                    self.assertEqual([item['alias'] for item in choices['items']],['codex'])
                    capability=api.get(base+'/factory-profile-capabilities/codex')
                    self.assertTrue(capability['model_catalog']['available'])
                    models=capability['model_catalog']['models']; self.assertGreaterEqual(len(models),2)
                    role=api.post(base+'/factory-role-publications',namespace+'-role',{'role':{'id':str(uuid.uuid4()),'expected_current_version':None,'definition':{'name':'Runtime qualification agent','instructions':['Follow Project rules.']}},'profiles':[]})['role']
                    expected=[]
                    for index,name in enumerate(('default','pro')):
                        definition=__import__('copy').deepcopy(capability['definition']); definition.update(name=name,model_identifier=models[index]['identifier'])
                        profile_id=str(uuid.uuid4())
                        validation=api.post(base+'/factory-role-profiles/validate',namespace+'-validate-'+name,{'id':profile_id,'role':role['reference'],'capability':'codex','definition':definition})
                        self.assertIsNone(validation['incompatibility'])
                        publication=api.post(base+'/factory-role-publications',namespace+'-'+name,{'profiles':[{'id':profile_id,'role':role['reference'],'expected_current_version':None,'definition':validation['profile']['definition']}]})
                        expected.append(publication['profiles'][0])
                    profiles_path=base+'/factory-roles/'+role['reference']['id']+'/profiles?limit=25'
                    self.assertEqual({p['name'] for p in api.get(profiles_path)['items']},{'default','pro'})
                    api.post(base+'/factory-profile-capabilities/codex/refresh',namespace+'-refresh',{})
                    for profile in expected:
                        actual=api.get(base+'/factory-role-profiles/'+profile['reference']['id'])
                        self.assertEqual(actual,profile)
                    self.assertEqual(api.get(base+'/builds?limit=25')['items'],[])
            finally:
                if server:
                    log = subprocess.run(['docker','logs',server],capture_output=True,text=True,check=False)
                    Path('/tmp/factory-runtime-qualified-server.log').write_text(log.stdout+log.stderr)
                    subprocess.run(['docker','stop','--time','5',server],capture_output=True,check=False)
                    subprocess.run(['docker','rm','--force',server],capture_output=True,check=False)
                launcher._compose_down(root,8443,remove_volumes=True)
