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
COMPANION_POLICY = POLICY.with_name('factory-runtime-companions.json')
DISCOVERY_PROGRAM = '/usr/local/bin/octa_plugin_codex'

def _native_login_bytes(path: Path) -> bytes:
    """Validate selection only; the pinned native client owns the credential format."""
    try:
        contents = state.read_regular_file(path, 'native login', max_bytes=65536,
            modes=frozenset({0o600}))
        value = json.loads(contents)
        tokens = value.get('tokens')
        if (value.get('auth_mode') != 'chatgpt' or value.get('OPENAI_API_KEY') not in (None, '')
                or not isinstance(tokens, dict)
                or any(not isinstance(tokens.get(name), str) or not tokens[name]
                       for name in ('access_token', 'refresh_token'))):
            raise ValueError('unsupported authentication')
        return contents
    except (OSError, UnicodeError, ValueError, AttributeError, state.StateError):
        raise ValueError('native ChatGPT login installation is invalid') from None


def install_native_login(root: Path, *, home: Path | None = None) -> dict:
    """Seed the existing private credential owner once; never restore stale seeds.

    Native refresh updates this stand-owned file through the Agent installation.
    Workstation configuration, MCP connections and session history are not copied.
    """
    root = root.expanduser().resolve()
    target = root / 'secrets/factory-model-login/auth.json'
    if target.exists() or target.is_symlink():
        _native_login_bytes(target)
    else:
        operator = home or Path(os.environ.get('CODEX_HOME', str(Path.home() / '.codex')))
        contents = _native_login_bytes(operator / 'auth.json')
        state.ensure_private_directory(root)
        state.ensure_private_directory(root / 'secrets')
        state.ensure_private_directory(target.parent)
        if not state.publish_exclusive_private_file(target, contents):
            _native_login_bytes(target)
    return {'path': str(target), 'destination': 'codex/auth.json',
            'sensitive_fields': ['/tokens'], 'persist_updates': True}

def _digest(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()

def install_runtime_companions(runtime: dict) -> list[dict]:
    """Install pinned peer executables under the existing private Runtime owner.

    Each peer remains a separate executable inventory/permission entry. No login,
    model settings or workload authority is installed here.
    """
    policy_bytes = COMPANION_POLICY.read_bytes()
    policy = json.loads(policy_bytes)
    if policy['runtime_version'] != runtime['policy']['archive']['version']:
        raise ValueError('runtime companion version differs from the selected executable')
    bundle = runtime['bundle']
    state.validate_private_directory(bundle)
    receipt = bundle / 'companions.json'
    identity = _digest(policy_bytes)
    if receipt.exists() or receipt.is_symlink():
        retained = json.loads(state.read_regular_file(receipt, 'runtime companions', max_bytes=65536,
            modes=frozenset({0o600})))
        if retained['policy_sha256'] != identity:
            raise ValueError('runtime companion installation differs from the reviewed policy')
        selected = retained['executables']
    else:
        selected = []
        for item in policy['executables']:
            source = item['archive']
            name = item['filename']
            if (source['version'] != policy['runtime_version'] or Path(name).name != name
                    or name in ('', '.', '..', 'codex', 'inputs.json', 'companions.json')):
                raise ValueError('invalid runtime companion identity')
            with tempfile.TemporaryDirectory(prefix='.companion-', dir=bundle) as temporary:
                staging = Path(temporary)
                archive = staging / 'release.tar.gz'
                download(source, archive)
                extract_tar_archive(archive, staging / 'release', max_expanded_bytes=source['expanded_max_bytes'],
                    max_file_bytes=source['file_max_bytes'], max_members=source['member_max_count'])
                payload = (staging / 'release' / source['executable']).read_bytes()
                target = bundle / name
                if state.publish_exclusive_private_file(target, payload):
                    target.chmod(0o700)
                actual = state.read_regular_file(target, 'runtime companion', max_bytes=source['file_max_bytes'],
                    modes=frozenset({0o700}))
                if _digest(actual) != _digest(payload):
                    raise ValueError('runtime companion bytes differ from the reviewed installation')
                selected.append({'identity':item['identity'], 'version':source['version'], 'platform':item['platform'],
                    'path':str(target), 'sha256':_digest(actual), 'guest_filename':name})
        state.publish_exclusive_private_file(receipt, json.dumps({'policy_sha256':identity, 'executables':selected}).encode())
    for item in selected:
        actual = state.read_regular_file(Path(item['path']), 'runtime companion', max_bytes=256 * 1024**2,
            modes=frozenset({0o700}))
        if _digest(actual) != item['sha256']:
            raise ValueError('runtime companion bytes differ from the reviewed installation')
    return selected

def _settings_support(compatibility: dict) -> dict:
    # The released adapter owns these transport families; no MCP program or grant is installed.
    qualified = CODEX_RUNTIME_SETTINGS_CAPABILITY in compatibility.get('plugin', {}).get('capabilities', [])
    return {'mcp_transports':['http','stdio'] if qualified else [], 'mcp_executables':{}}

def install_runtime_executable(root: Path, native: Path, *, octa_root: Path | None = None) -> dict:
    """Install verified executable bytes independently of login and model discovery."""
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
            state.publish_exclusive_private_file(prepared / 'inputs.json', json.dumps({
                'policy_sha256':_digest(policy_bytes), 'executable_sha256':_digest(cli)}).encode())
            prepared.rename(bundle)
    state.validate_private_directory(bundle)
    retained = json.loads(state.read_regular_file(inputs, 'Runtime identity', max_bytes=4096, modes=frozenset({0o600})))
    program = bundle / 'codex'
    cli = state.read_regular_file(program, 'installed Codex runtime', max_bytes=source['file_max_bytes'], modes=frozenset({0o700}))
    if retained != {'policy_sha256':_digest(policy_bytes), 'executable_sha256':_digest(cli)}:
        raise ValueError('installed runtime identity differs from the reviewed installation')
    runtime = {'bundle': bundle, 'executable': program, 'sha256': _digest(cli),
               'policy': policy, 'compatibility': compatibility, 'octa': installed}
    runtime['companions'] = install_runtime_companions(runtime)
    return runtime


def _install_discovery_home(bundle: Path, home: Path | None) -> None:
    """Keep the explicit legacy discovery snapshot separate from executable installation."""
    destination = bundle / 'home'
    if destination.exists() or destination.is_symlink():
        state.validate_private_directory(destination)
        return
    with tempfile.TemporaryDirectory(prefix='.discovery-', dir=bundle) as temporary:
        prepared = Path(temporary) / 'home'
        auth_root = prepared / '.codex'
        state.ensure_private_directory(prepared)
        state.ensure_private_directory(auth_root)
        operator = home or Path(os.environ.get('CODEX_HOME', str(Path.home() / '.codex')))
        auth_file = operator / 'auth.json'
        if auth_file.exists() or auth_file.is_symlink():
            contents = state.read_regular_file(auth_file, 'Codex authentication', max_bytes=65536, modes=frozenset({0o600}))
            state.publish_exclusive_private_file(auth_root / 'auth.json', contents)
        prepared.rename(destination)


def install_runtime(root: Path, native: Path, resources: dict, client, *, home: Path | None = None, octa_root: Path | None = None) -> dict:
    """Install the legacy discovery catalog without creating execution grants."""
    runtime = install_runtime_executable(root, native, octa_root=octa_root)
    bundle, policy = runtime['bundle'], runtime['policy']
    source, installed, compatibility = policy['archive'], runtime['octa'], runtime['compatibility']
    _install_discovery_home(bundle, home)
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
    executable = exact('codex-cli',source['version'],runtime['sha256'])
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
