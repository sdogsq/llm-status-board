"""Publish complete installers; assemble signed metadata after all matrix jobs finish."""
import argparse
import base64
import hashlib
import json
import os
import re
from pathlib import Path
import subprocess
import tempfile
from urllib.parse import quote


def gh(*args):
    return subprocess.check_output(['gh', *args], text=True)


def installers(assets, version):
    expected = [f'LLM.Status.Board_{version}_x64-setup.exe',
                f'LLM.Status.Board_{version}_aarch64.dmg',
                f'LLM.Status.Board_{version}_x64.dmg']
    for name in expected:
        if name not in assets or assets[name]['state'] != 'uploaded' or assets[name]['size'] <= 0:
            raise ValueError(f'Missing or incomplete installer: {name}')
    return expected


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--repo', required=True)
    parser.add_argument('--tag', required=True)
    parser.add_argument('--channel', choices=['stable', 'nightly'], default='stable')
    parser.add_argument('--version', help='Explicit nightly version; stable versions come from the tag')
    parser.add_argument('--dry-run', action='store_true')
    args = parser.parse_args()
    # The releases/tags endpoint can return 404 for drafts; the list endpoint includes them.
    matches = [release for release in json.loads(gh('api', f'repos/{args.repo}/releases?per_page=100'))
               if release['tag_name'] == args.tag]
    if len(matches) != 1:
        raise ValueError('Expected exactly one matching release or draft')
    release = matches[0]
    if not args.dry_run and not release['draft']:
        raise ValueError('Refusing to mutate an already published release')
    assets = {asset['name']: asset for asset in release['assets']}
    nightly = args.channel == 'nightly'
    if nightly:
        if args.tag != 'nightly' or not release['prerelease'] or not re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+-nightly\.[0-9]{8}\.[0-9]+', args.version or ''):
            raise ValueError('Nightly requires its own prerelease tag and version')
        version = args.version
    else:
        if not re.fullmatch(r'v[0-9]+\.[0-9]+\.[0-9]+', args.tag) or args.version:
            raise ValueError('Stable releases require a version tag without an override')
        version = args.tag.removeprefix('v')
    expected = installers(assets, version)
    signed = os.environ.get('SIGNED_UPDATER') == 'true'
    if nightly and not signed:
        raise ValueError('Nightly requires signed updater artifacts')
    notes = release['body'] if nightly else Path('.github/release-notes.md').read_text()
    notes += '\n\n更新方式：' + ('支持应用内签名更新。' if signed else '本安装包使用手动更新，请从本仓库 Releases 下载；尚未配置 fork 签名自动更新。')
    with tempfile.TemporaryDirectory() as folder:
        root = Path(folder)

        def download(name):
            path = root / name
            if not path.exists():
                gh('release', 'download', args.tag, '--repo', args.repo,
                   '--pattern', name, '--dir', str(root))
            asset = assets[name]
            if path.stat().st_size != asset['size']:
                raise ValueError(f'Wrong file size: {name}')
            digest = 'sha256:' + hashlib.sha256(path.read_bytes()).hexdigest()
            if digest != asset.get('digest'):
                raise ValueError(f'SHA256 mismatch: {name}')
            return path

        for name in expected:
            download(name)
            print(f'Verified {name}', flush=True)
        if signed:
            public = root / 'updater.pub'
            public.write_bytes(base64.b64decode(os.environ['UPDATER_PUBLIC_KEY'], validate=True))
            platforms = {}
            for platform, name in [('windows-x86_64', expected[0]),
                                   ('darwin-aarch64', 'LLM.Status.Board_aarch64.app.tar.gz'),
                                   ('darwin-x86_64', 'LLM.Status.Board_x64.app.tar.gz')]:
                payload = download(name)
                signature = download(name + '.sig').read_text().strip()
                decoded = root / f'{platform}.minisig'
                decoded.write_bytes(base64.b64decode(signature, validate=True))
                subprocess.run(['minisign', '-Vm', str(payload), '-p', str(public), '-x', str(decoded)], check=True)
                # Draft asset URLs contain an untagged-* path that changes at publication.
                url = f'https://github.com/{args.repo}/releases/download/{quote(args.tag, safe="")}/{quote(name, safe="")}'
                platforms[platform] = {'url': url, 'signature': signature}
            manifest = root / 'latest.json'
            manifest.write_text(json.dumps({'version': version, 'notes': notes,
                                           'pub_date': release['created_at'], 'platforms': platforms}, indent=2))
            if not args.dry_run:
                gh('release', 'upload', args.tag, str(manifest), '--repo', args.repo, '--clobber')
        if args.dry_run:
            print('Dry run complete; no release changes made.')
            return
        notes_file = root / 'notes.md'
        notes_file.write_text(notes)
        gh('release', 'edit', args.tag, '--repo', args.repo, '--title', 'nightly' if nightly else f'LLM Status Board {args.tag}',
           '--notes-file', str(notes_file), '--draft=false', '--prerelease' if nightly else '--prerelease=false',
           '--latest=false' if nightly else '--latest')
        print(f'Published https://github.com/{args.repo}/releases/tag/{args.tag}')


if __name__ == '__main__':
    main()
