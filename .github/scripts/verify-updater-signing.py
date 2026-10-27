"""Verify Actions updater credentials using a disposable payload; never publish keys."""
import base64
import hashlib
import os
from pathlib import Path
import subprocess
import tempfile


def run(command, env=None):
    return subprocess.run(command, env=env, capture_output=True, timeout=60)


def main():
    private = os.environ.get('TAURI_SIGNING_PRIVATE_KEY', '').strip()
    public = os.environ.get('TAURI_SIGNING_PUBLIC_KEY', '').strip()
    if not private or not public:
        raise SystemExit('Missing Actions Secret TAURI_SIGNING_PRIVATE_KEY or Variable TAURI_SIGNING_PUBLIC_KEY')
    decoded = {}
    invalid = False
    for name, value in [('TAURI_SIGNING_PRIVATE_KEY', private), ('TAURI_SIGNING_PUBLIC_KEY', public)]:
        try:
            decoded[name] = base64.b64decode(value, validate=True)
            print(name + ': encoded format OK', flush=True)
        except ValueError as error:
            invalid = True
            print(name + ': invalid encoded format', flush=True)
            if 'Incorrect padding' in str(error) or 'number of data characters' in str(error):
                print('  Encoded text is incomplete or has missing Base64 padding.', flush=True)
            else:
                print('  Value contains characters outside the standard Base64 alphabet.', flush=True)
            if any(char in value for char in ['"', "'", '`']):
                print('  Quote or code-fence characters are present; paste only file contents.', flush=True)
            if '...' in value or '\u2026' in value:
                print('  Truncation dots are present; use the complete original file.', flush=True)
            if any(char.isspace() for char in value):
                print('  Contains whitespace inside the encoded value; copy its single complete line.', flush=True)
            if any(ord(char) > 127 for char in value):
                print('  Contains non-ASCII text; copy only the encoded key file contents.', flush=True)
    if invalid:
        raise SystemExit('Replace the invalid configuration entry with complete Tauri key file contents without decoding')
    public_bytes = decoded['TAURI_SIGNING_PUBLIC_KEY']

    env = os.environ.copy()
    env['CI'] = 'true'
    env['TAURI_SIGNING_PRIVATE_KEY'] = private
    env.pop('TAURI_SIGNING_PRIVATE_KEY_PATH', None)
    cli = Path('node_modules/@tauri-apps/cli/tauri.js').resolve()
    with tempfile.TemporaryDirectory(prefix='verify-updater-') as folder:
        root = Path(folder)
        payload = root / 'signing-probe.bin'
        payload.write_bytes(os.urandom(64))
        command = ['node', str(cli), 'signer', 'sign', str(payload)]
        if run(command, env).returncode:
            raise SystemExit('Signing failed: check the private key and its password; signer output was suppressed')
        print('PASS: Actions private key and password sign successfully', flush=True)

        public_path = root / 'updater.pub'
        public_path.write_bytes(public_bytes)
        signature = root / 'probe.minisig'
        try:
            signature.write_bytes(base64.b64decode(Path(str(payload) + '.sig').read_text().strip(), validate=True))
        except (ValueError, OSError):
            raise SystemExit('Signer did not produce a valid encoded Minisign signature')
        verification = ['minisign', '-Vm', str(payload), '-p', str(public_path), '-x', str(signature)]
        if run(verification).returncode:
            raise SystemExit('Verification failed: the public key must match the private key')
        print('PASS: Actions public key verifies the signed payload', flush=True)

        payload.write_bytes(payload.read_bytes() + b'modified')
        if run(verification).returncode == 0:
            raise SystemExit('Verification incorrectly accepted a modified payload')
        print('PASS: modified payload is rejected', flush=True)
        print('Public key SHA256: ' + hashlib.sha256(public_bytes).hexdigest(), flush=True)
        print('Signed updater configuration verified; no release was created or changed.', flush=True)


if __name__ == '__main__':
    main()
