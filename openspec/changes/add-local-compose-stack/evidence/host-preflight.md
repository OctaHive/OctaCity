# Native Microsandbox host preflight

## Result

**PASS:** the target Apple Silicon Mac satisfies the native Microsandbox 0.7.6
host preflight. The official macOS ARM64 bundle matched its published SHA-256,
both executable components passed strict code-signature verification, and
`msb doctor` reported the host setup ready.

The rejected container/KVM experiment is retained below because it explains
why the local stand uses a native Agent alongside its OrbStack service plane.

Observed on `2026-10-02`.

## Host

- Hardware: `MacBookPro18,1`, Apple Silicon ARM64
- Operating system: macOS `27.0.1` (`26A434`)
- Kernel: `Darwin 27.0.0`, `RELEASE_ARM64_T6000`

## Native runtime

- Release: Microsandbox `0.7.6`
- Asset: `microsandbox-darwin-aarch64.tar.gz`
- Published and observed SHA-256: `a4f1722fee6460d4e2813e62b60dae4c092021d8120b7aaa9c2f4b5679d97d89`
- Executable probe: `msb 0.7.6`
- Runtime signature: valid and satisfies its designated requirement
- `libkrunfw.5.dylib` signature: valid and satisfies its designated requirement
- Preflight state used a disposable `MSB_HOME`; the existing user installation
  at `~/.microsandbox` was not modified

Command shape:

```console
MSB_HOME=<disposable-state> \
MSB_LIBKRUNFW_PATH=<verified-runtime>/lib/libkrunfw.5.dylib \
<verified-runtime>/bin/msb doctor
```

Output:

```text
info Platform: macOS aarch64
info Version: v0.7.6
   ✓ msb          <verified-runtime>/bin/msb
   ✓ libkrunfw    <verified-runtime>/lib/libkrunfw.5.dylib
   ✓ Root clone   reflink supported
   ✓ Architecture Apple silicon (arm64)
done Host setup is ready.
```

## Rejected OrbStack container path

OrbStack `2.2.3` provides a Docker-compatible `29.4.0` Linux ARM64 engine, but
it rejected the pinned official Microsandbox container's `/dev/kvm` mapping
before `doctor` could start:

```text
docker: Error response from daemon: error gathering device information while
adding custom device "/dev/kvm": no such file or directory
```

This is consistent with OrbStack's current platform documentation, which says
nested KVM virtualization is not supported on Apple Silicon:
<https://docs.orbstack.dev/machines/#nested-virtualization>.

The native Agent is therefore a required architectural boundary, not a
fallback. The launcher must preserve the passing native preflight and must not
attempt a privileged container or register a weaker execution provider.
