"""Install a qualified Codex catalog using the shared private executable bundle."""
from __future__ import annotations
import copy
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

import local_stand_state as state
from local_stand_launcher import SERVER_IMAGE
from pinned_source_archive import download, extract_tar_archive
from validate_octa_release_contract import CODEX_RUNTIME_SETTINGS_CAPABILITY

POLICY = Path(__file__).resolve().parents[1] / 'deployment/local-stand/factory-runtime.json'
DISCOVERY_PROGRAM = '/usr/local/bin/octa_plugin_codex'

def _digest(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()

def _settings_support(compatibility: dict) -> dict:
    # The released adapter owns these transport families; no MCP program or grant is installed.
    qualified = CODEX_RUNTIME_SETTINGS_CAPABILITY in compatibility.get('plugin', {}).get('capabilities', [])
    return {'mcp_transports':['http','stdio'] if qualified else [], 'mcp_executables':{}}

def install_runtime(root: Path, native: Path, resources: dict, client, *, home: Path | None = None, octa_root: Path | None = None) -> dict:
    """Install bytes and compatibility, never creating a Role, task or execution grant."""
    policy_bytes = POLICY.read_bytes()
    policy = json.loads(policy_bytes)
    source = policy['archive']
    octa = octa_root or native / 'octa'
    compatibility = json.loads((octa / 'codex-compatibility.json').read_text())
    if source['version'] not in compatibility['executable']['supported_versions']:
        raise ValueError('selected Codex runtime is not supported by the installed plugin')
    installed = json.loads((native / 'job-spec-policy.json').read_text())['octa']
    bundle = root / 'factory-runtime'
    state.ensure_private_directory(root)
    inputs = bundle / 'inputs.json'
    if not bundle.exists():
        with tempfile.TemporaryDirectory(prefix='.runtime-', dir=root) as temporary:
            staging = Path(temporary)
            archive = staging / 'runtime.tar.gz'
            download(source, archive)
            extract_tar_archive(archive, staging / 'release',
                max_expanded_bytes=source['expanded_max_bytes'], max_file_bytes=source['file_max_bytes'],
                max_members=source['member_max_count'])
            prepared = staging / 'bundle'
            state.ensure_private_directory(prepared)
            program = prepared / 'codex'
            shutil.copyfile(staging / 'release' / source['executable'], program)
            program.chmod(0o700)
            cli = state.read_regular_file(program, 'installed Codex runtime', max_bytes=source['file_max_bytes'], modes=frozenset({0o700}))
            private_home = prepared / 'home'
            state.ensure_private_directory(private_home)
            auth_root = private_home / '.codex'
            state.ensure_private_directory(auth_root)
            operator = home or Path(os.environ.get('CODEX_HOME', str(Path.home() / '.codex')))
            auth_file = operator / 'auth.json'
            if auth_file.exists() or auth_file.is_symlink():
                contents = state.read_regular_file(auth_file, 'Codex authentication', max_bytes=65536, modes=frozenset({0o600}))
                state.publish_exclusive_private_file(auth_root / 'auth.json', contents)
            state.publish_exclusive_private_file(prepared / 'inputs.json', json.dumps({
                'policy_sha256':_digest(policy_bytes), 'executable_sha256':_digest(cli)}).encode())
            prepared.rename(bundle)
    state.validate_private_directory(bundle)
    retained = json.loads(state.read_regular_file(inputs, 'Runtime identity', max_bytes=4096, modes=frozenset({0o600})))
    program = bundle / 'codex'
    cli = state.read_regular_file(program, 'installed Codex runtime', max_bytes=source['file_max_bytes'], modes=frozenset({0o700}))
    if retained != {'policy_sha256':_digest(policy_bytes), 'executable_sha256':_digest(cli)}:
        raise ValueError('installed runtime identity differs from the reviewed installation')
    cached = bundle / 'project.json'
    if cached.exists():
        project = json.loads(state.read_regular_file(cached, 'Runtime project', max_bytes=131072, modes=frozenset({0o600})))
        if project['project_id'] != resources['project']:
            raise ValueError('installed Runtime belongs to another Project')
        project['role_profiles']['codex']['settings_support'] = _settings_support(compatibility)
        return {'bundle':bundle,'project':project}
    # The server image builds this native discovery plugin from the same pinned Octa source.
    result = subprocess.run(['docker','run','--rm','--network','none','--read-only',
        '--entrypoint','sha256sum',SERVER_IMAGE,DISCOVERY_PROGRAM],
        check=True,capture_output=True,text=True,timeout=20)
    discovery_digest = result.stdout.split()[0]
    if len(discovery_digest) != 64 or any(c not in '0123456789abcdef' for c in discovery_digest):
        raise ValueError('invalid discovery plugin identity')
    exact = lambda name, version, digest: {'identity':name,'version':version,'digest':digest}
    plugin = exact('codex',installed['version'],installed['plugin_digests']['codex'])
    executable = exact('codex-cli',source['version'],_digest(cli))
    model_contract = exact('codex-model-catalog','v1',_digest(json.dumps(plugin,sort_keys=True).encode()))
    configuration = client.get(f"/api/v1/build-configurations/{resources['configuration']}/versions/1")
    build = {k:configuration[k] for k in ('id','version','project_id')}
    build['definition_digest'] = _digest(json.dumps(configuration['definition'],sort_keys=True,separators=(',',':'),ensure_ascii=False).encode())
    parameters = copy.deepcopy(policy['parameters'])
    declaration = {'ceiling':{'name':'Codex','instructions':[], 'toolchain':{'plugin':plugin,'executable':executable,'model':model_contract},
        'model_parameters':{},'skills':{},'context_requirements':[], 'build_configuration':build,'credential_purpose':'model',
        'permissions':copy.deepcopy(policy['permissions']),'budget':copy.deepcopy(policy['budget'])},
        'parameters':parameters,'public_environment':{},'enforcement':copy.deepcopy(policy['enforcement']),
        'settings_support':_settings_support(compatibility),
        'model_discovery':{'executable':DISCOVERY_PROGRAM,'executable_sha256':discovery_digest,
            'environment':{compatibility['executable']['selection_environment']:'/run/octacity-runtime/codex','HOME':'/run/octacity-runtime/home'}}}
    references = [{'kind':'node_capability','alias':'codex','reference':executable}]
    references += [{'kind':'node_capability','alias':f'codex-{kind}','reference':ref} for kind,ref in (('plugin',plugin),('models',model_contract))]
    project = {'project_id':resources['project'],'references':references,
        'build_configurations':[{'alias':'codex-runtime','reference':build}], 'role_profiles':{'codex':declaration}}
    state.publish_exclusive_private_file(cached,json.dumps(project,sort_keys=True,separators=(',',':')).encode())
    return {'bundle':bundle,'project':project}
