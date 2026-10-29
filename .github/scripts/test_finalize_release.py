"""Check stable/nightly publication policy without network access or real releases."""
import base64
import contextlib
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('finalize_release', Path(__file__).with_name('finalize-release.py'))
release_script = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release_script)


class PublicationTests(unittest.TestCase):
    def fixture(self, nightly=False):
        self.tag = 'nightly' if nightly else 'v0.2.15'
        self.version = '0.2.15-nightly.20261029.7' if nightly else '0.2.15'
        self.calls = []
        self.manifest = None
        self.notes = None
        self.payloads = {
            f'LLM.Status.Board_{self.version}_x64-setup.exe': b'windows installer',
            f'LLM.Status.Board_{self.version}_aarch64.dmg': b'arm installer',
            f'LLM.Status.Board_{self.version}_x64.dmg': b'intel installer',
            'LLM.Status.Board_aarch64.app.tar.gz': b'arm updater',
            'LLM.Status.Board_x64.app.tar.gz': b'intel updater',
        }
        for name in list(self.payloads):
            if name.endswith(('.exe', '.tar.gz')):
                self.payloads[name + '.sig'] = base64.b64encode(b'test signature')
        self.release = {
            'tag_name': self.tag, 'draft': True, 'prerelease': nightly,
            'created_at': '2026-10-29T10:00:00Z',
            'body': 'Nightly notes\n<!-- nightly-commit:' + 'a' * 40 + ' -->',
            'assets': [{'name': name, 'state': 'uploaded', 'size': len(data),
                        'digest': 'sha256:' + hashlib.sha256(data).hexdigest()}
                       for name, data in self.payloads.items()],
        }

    def gh(self, *args):
        self.calls.append(args)
        if args[0] == 'api':
            return json.dumps([self.release])
        if args[:2] == ('release', 'download'):
            name = args[args.index('--pattern') + 1]
            folder = args[args.index('--dir') + 1]
            (Path(folder) / name).write_bytes(self.payloads[name])
        elif args[:2] == ('release', 'upload'):
            self.manifest = json.loads(Path(args[3]).read_text())
        elif args[:2] == ('release', 'edit'):
            self.notes = Path(args[args.index('--notes-file') + 1]).read_text()
        else:
            self.fail(f'Unexpected gh call: {args}')
        return ''

    def run_publication(self, *extra, verifier_error=None, signed=True):
        args = ['finalize-release.py', '--repo', 'sdogsq/llm-status-board', '--tag', self.tag]
        if self.tag == 'nightly':
            args += ['--channel', 'nightly', '--version', self.version]
        args += extra
        with patch('sys.argv', args), patch.dict(os.environ, {
            'SIGNED_UPDATER': str(signed).lower(), 'UPDATER_PUBLIC_KEY': base64.b64encode(b'test public key').decode(),
        }), patch.object(release_script, 'gh', side_effect=self.gh), \
                patch.object(release_script.subprocess, 'run', side_effect=verifier_error) as verifier, \
                contextlib.redirect_stdout(io.StringIO()):
            release_script.main()
        return verifier

    def mutations(self):
        return [call for call in self.calls if call[:2] in [('release', 'upload'), ('release', 'edit')]]

    def test_stable_verifies_all_signatures_before_publishing_latest(self):
        self.fixture()
        verifier = self.run_publication()
        self.assertEqual(verifier.call_count, 3)
        self.assertEqual(self.manifest['version'], self.version)
        self.assertEqual(self.manifest['notes'], self.notes)
        self.assertTrue(self.notes.startswith(Path('.github/release-notes.md').read_text()))
        self.assertEqual(set(self.manifest['platforms']), {'windows-x86_64', 'darwin-aarch64', 'darwin-x86_64'})
        for item in self.manifest['platforms'].values():
            self.assertIn('/releases/download/v0.2.15/', item['url'])
        self.assertIn('--latest', self.mutations()[-1])
        self.assertIn('--prerelease=false', self.mutations()[-1])

    def test_nightly_preserves_commit_notes_and_cannot_replace_stable_latest(self):
        self.fixture(nightly=True)
        verifier = self.run_publication()
        self.assertEqual(verifier.call_count, 3)
        self.assertEqual(self.manifest['version'], self.version)
        self.assertTrue(self.notes.startswith(self.release['body']))
        self.assertEqual(self.notes, self.manifest['notes'])
        for item in self.manifest['platforms'].values():
            self.assertIn('/releases/download/nightly/', item['url'])
        publication = self.mutations()[-1]
        self.assertIn('--prerelease', publication)
        self.assertIn('--latest=false', publication)
        self.assertNotIn('--latest', publication)

    def test_missing_platform_or_bad_digest_prevents_any_publication(self):
        for failure in ['missing', 'digest']:
            with self.subTest(failure=failure):
                self.fixture()
                if failure == 'missing':
                    self.release['assets'].pop(0)
                else:
                    self.release['assets'][0]['digest'] = 'sha256:' + '0' * 64
                with self.assertRaises(ValueError):
                    self.run_publication()
                self.assertFalse(self.mutations())

    def test_bad_signature_prevents_manifest_upload_and_publication(self):
        self.fixture(nightly=True)
        with self.assertRaises(subprocess.CalledProcessError):
            self.run_publication(verifier_error=subprocess.CalledProcessError(1, 'minisign'))
        self.assertFalse(self.mutations())

    def test_nightly_cannot_use_an_unsigned_build_or_stable_tag(self):
        self.fixture(nightly=True)
        with self.assertRaises(ValueError):
            self.run_publication(signed=False)
        self.assertFalse(self.mutations())
        self.fixture()
        with self.assertRaises(ValueError):
            self.run_publication('--channel', 'nightly', '--version', '0.2.15-nightly.20261029.7')
        self.assertFalse(self.mutations())

    def test_public_release_is_immutable_and_dry_run_never_writes(self):
        self.fixture()
        self.release['draft'] = False
        with self.assertRaises(ValueError):
            self.run_publication()
        self.assertFalse(self.mutations())
        self.run_publication('--dry-run')
        self.assertFalse(self.mutations())


if __name__ == '__main__':
    unittest.main()
