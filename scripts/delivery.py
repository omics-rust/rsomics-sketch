"""GitHub source/native delivery; never a Cargo registry package."""

import argparse
import csv
import gzip
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import tarfile

PRODUCT = 'rsomics-sketch'
ROOT = Path(__file__).resolve().parents[1]
KMER = 'c79111f31651bad581920b7b5c2ddde9a57534e1'
HOSTS = {'linux-x86_64': 'x86_64-unknown-linux-gnu', 'linux-aarch64': 'aarch64-unknown-linux-gnu',
         'macos-x86_64': 'x86_64-apple-darwin', 'macos-aarch64': 'aarch64-apple-darwin'}


def identity(path):
    body = Path(path).read_bytes()
    return dict(bytes=len(body), sha256=hashlib.sha256(body).hexdigest())


def save(path, value):
    Path(path).write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')


def canonical(value):
    return (json.dumps(value, indent=2, sort_keys=True) + '\n').encode()


def require(ok, message):
    if not ok:
        raise ValueError(message)


def source_manifest():
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    require(commit == os.environ['GITHUB_SHA'], 'source commit differs from the actual CI head')
    require(subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT) == b'', 'source worktree is dirty')
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--locked', '--no-deps', '--format-version', '1'], cwd=ROOT))
    package, = [p for p in metadata['packages'] if p['name'] == PRODUCT]
    records = subprocess.check_output(['git', 'ls-tree', '-rz', '--full-tree', commit], cwd=ROOT)
    files = {}
    for record in records.rstrip(b'\0').split(b'\0'):
        header, name = record.split(b'\t', 1)
        mode, kind, blob = header.decode().split()
        name = name.decode()
        require(kind == 'blob' and mode in ('100644', '100755'), 'unsupported Git source member: ' + name)
        body = (ROOT / name).read_bytes()
        observed_blob = hashlib.sha1(b'blob ' + str(len(body)).encode() + b'\0' + body).hexdigest()
        require(observed_blob == blob, 'worktree body differs from exact Git source: ' + name)
        require(bool((ROOT / name).stat().st_mode & 0o111) == (mode == '100755'), 'source mode differs: ' + name)
        files[name] = dict(bytes=len(body), sha256=hashlib.sha256(body).hexdigest(), git_blob=blob, git_mode=mode)
    require(package['version'] == '0.1.1' and package['publish'] == [], 'unexpected corrected package boundary')
    return dict(schema='rsomics.github-source.v1', product=PRODUCT, version=package['version'],
        prefix=PRODUCT + '-' + package['version'] + '-source', source_commit=commit,
        source_tree=subprocess.check_output(['git', 'rev-parse', commit + '^{tree}'], cwd=ROOT, text=True).strip(),
        files=files, registry_package=False, dependencies_vendored=False,
        dependency_policy='Original Cargo.toml/Cargo.lock preserve public pinned Git dependencies; fresh builds need network/cache access.')


def write_source_tar(root, manifest, output):
    root, output = Path(root), Path(output)
    bodies = {}
    for name, row in manifest['files'].items():
        body = (root / name).read_bytes()
        require(dict(bytes=len(body), sha256=hashlib.sha256(body).hexdigest()) == {k: row[k] for k in ('bytes', 'sha256')},
                'original source body changed: ' + name)
        bodies[name] = body
    bodies['SOURCE.json'] = canonical(manifest)
    directories = {manifest['prefix']}
    for name in bodies:
        path = PurePosixPath(manifest['prefix']) / name
        require(not path.is_absolute() and '..' not in path.parts, 'unsafe source path')
        directories.update(str(p) for p in path.parents if str(p) != '.')
    with output.open('xb') as stream, tarfile.open(fileobj=stream, mode='w', format=tarfile.PAX_FORMAT) as archive:
        for name in sorted(directories):
            row = tarfile.TarInfo(name)
            row.type, row.mode, row.mtime = tarfile.DIRTYPE, 0o755, 0
            archive.addfile(row)
        for name, body in sorted(bodies.items()):
            row = tarfile.TarInfo(manifest['prefix'] + '/' + name)
            row.size, row.mtime = len(body), 0
            row.mode = int(manifest['files'][name]['git_mode'], 8) & 0o777 if name != 'SOURCE.json' else 0o644
            archive.addfile(row, io.BytesIO(body))


def extract_source_tar(archive_path, manifest, destination):
    destination = Path(destination)
    destination.mkdir()
    with tarfile.open(archive_path, 'r:') as archive:
        members = archive.getmembers()
        require(len(members) == len({m.name for m in members}), 'duplicate source TAR member')
        require(len(members) == len({m.name.casefold() for m in members}), 'case-colliding source TAR member')
        original = manifest['files']
        expected = {manifest['prefix'] + '/' + name for name in [*original, 'SOURCE.json']}
        require({m.name for m in members if m.isfile()} == expected, 'source TAR original membership differs')
        require(archive.extractfile(manifest['prefix'] + '/SOURCE.json').read() == canonical(manifest), 'source provenance differs')
        for member in members:
            path = PurePosixPath(member.name)
            require(not path.is_absolute() and '..' not in path.parts and path.parts[0] == manifest['prefix'], 'unsafe source TAR path')
            require(member.isfile() or member.isdir(), 'unsupported source TAR type')
            require(member.uid == member.gid == member.mtime == 0 and member.uname == member.gname == '', 'source headers are not deterministic')
            target = destination / member.name
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True)
                target.chmod(0o755)
            else:
                body = archive.extractfile(member).read()
                name = '/'.join(path.parts[1:])
                if name in original:
                    require(dict(bytes=len(body), sha256=hashlib.sha256(body).hexdigest()) == {k: original[name][k] for k in ('bytes', 'sha256')},
                            'extracted original body differs: ' + name)
                    require(member.mode == int(original[name]['git_mode'], 8) & 0o777, 'extracted original mode differs: ' + name)
                target.parent.mkdir(parents=True, exist_ok=True)
                with target.open('xb') as stream:
                    stream.write(body)
                target.chmod(member.mode)
    return destination / manifest['prefix']


def source():
    temporary = Path(os.environ['RUNNER_TEMP'])
    delivery = temporary / 'delivery'
    delivery.mkdir(exist_ok=True)
    metadata_body = subprocess.check_output(['cargo', 'metadata', '--locked', '--format-version', '1'], cwd=ROOT)
    tree_body = subprocess.check_output(['cargo', 'tree', '--locked', '--target', os.environ['EXPECTED_HOST'],
        '--edges', 'normal,build', '--prefix', 'none', '--format', '{p}'], cwd=ROOT)
    (delivery / 'native-dependencies.metadata.json').write_bytes(metadata_body)
    (delivery / 'native-dependencies.tree.txt').write_bytes(tree_body)
    metadata = json.loads(metadata_body)
    by_name = {(p['name'], p['version']): p for p in metadata['packages']}
    enabled = set()
    for line in tree_body.decode().splitlines():
        name, version = re.match(r'([^ ]+) v([^ ]+)', line).groups()
        if name != PRODUCT:
            enabled.add(by_name[(name, version)]['id'])
    inventory = json.loads((ROOT / 'licenses/provenance.json').read_bytes())
    expected = set(inventory['target_graphs'][os.environ['EXPECTED_HOST']])
    require(enabled == expected,
        'actual native enabled dependency graph differs from the notice inventory')
    require(identity(ROOT / 'Cargo.lock')['sha256'] == inventory['lock_sha256'], 'notice inventory lock differs')
    kmer, = [p for p in metadata['packages'] if p['name'] == 'rsomics-kmer']
    require(kmer['version'] == '0.2.3' and kmer['source'] == 'git+https://github.com/omics-rust/rsomics-kmer?rev=' + KMER + '#' + KMER,
            'resolved foundation is not the exact corrected Git pin')
    for row in inventory['copied_texts'].values():
        require(identity(ROOT / row['destination']) == {k: row[k] for k in ('bytes', 'sha256')}, 'original dependency terms differ')
    save(delivery / 'native-dependencies.json', dict(target=os.environ['EXPECTED_HOST'], enabled_packages=sorted(enabled),
        git_foundation=dict(id=kmer['id'], source=kmer['source'], version=kmer['version'], commit=KMER),
        metadata_sha256=hashlib.sha256(metadata_body).hexdigest(), tree_sha256=hashlib.sha256(tree_body).hexdigest(),
        inventory_sha256=identity(ROOT / 'licenses/provenance.json')['sha256'],
        actual_native_enabled_graph_verified=True, linked_binary_inventory=False))
    manifest = source_manifest()
    archive = delivery / (manifest['prefix'] + '.tar')
    write_source_tar(ROOT, manifest, archive)
    extracted = extract_source_tar(archive, manifest, temporary / 'source')
    save(temporary / 'source-verification.json', dict(**identity(archive), name=archive.name,
        original_files=len(manifest['files']), source_commit=manifest['source_commit'], source_tree=manifest['source_tree'],
        all_original_bodies_modes_verified=True, deterministic_headers=True,
        registry_package=False, dependencies_vendored=False))
    with Path(os.environ['GITHUB_ENV']).open('a') as stream:
        stream.write('SOURCE_DIRECTORY=' + str(extracted) + '\n')
        stream.write('SOURCE_TAR=' + str(archive) + '\n')
        stream.write('PACKAGE_VERSION=' + manifest['version'] + '\n')


def smoke(binary):
    binary = Path(binary).resolve(strict=True)
    directory = Path(os.environ['RUNNER_TEMP']) / 'delivery/lifecycle'
    directory.mkdir(parents=True)
    shutil.copy2(binary, directory / 'installed-binary')
    records = []

    def run(arguments, status=0):
        command = [str(binary), *map(str, arguments)]
        index = len(records)
        records.append(dict(argv=command))
        save(directory / 'commands.json', records)
        with (directory / f'{index}.stdout').open('xb') as out, (directory / f'{index}.stderr').open('xb') as err:
            result = subprocess.run(command, cwd=directory, stdout=out, stderr=err,
                                    env=dict(os.environ, NO_COLOR='1'), timeout=120)
        records[-1]['returncode'] = result.returncode
        save(directory / 'commands.json', records)
        require(result.returncode == status, 'installed command exit differs: ' + str(command))
        return directory / f'{index}.stdout', directory / f'{index}.stderr'

    def envelope(arguments, operation):
        out, err = run(['--json', *arguments])
        value = json.loads(out.read_bytes())
        require(err.stat().st_size == 0 and value['schema_version'] == '1.0' and value['tool'] == PRODUCT and
                value['tool_version'] == os.environ['PACKAGE_VERSION'] and value['status'] == 'ok' and
                value['result']['operation'] == operation, 'installed common JSON envelope differs')
        return value['result']

    def rows(path):
        with path.open(newline='') as stream:
            return list(csv.DictReader(stream, delimiter='\t'))

    def literal(name, threshold, hashes, digest):
        return {
            'class': 'sourmash_signature', 'email': '', 'filename': name + '.fa', 'name': name,
            'license': 'CC0', 'hash_function': '0.murmur64', 'version': 0.4,
            'signatures': [dict(num=0, ksize=3, seed=42, max_hash=threshold, mins=hashes,
                                md5sum=digest, molecule='DNA')]}

    out, _ = run(['--version'])
    require(out.read_text().strip() == PRODUCT + ' ' + os.environ['PACKAGE_VERSION'], 'installed version differs')
    out, _ = run(['--help'])
    commands = out.read_text().split('Commands:\n', 1)[1].split('\n\n', 1)[0]
    require([line.split()[0] for line in commands.splitlines() if line.split()[0] != 'help'] ==
            ['sketch', 'inspect', 'compare', 'search'], 'installed four-command scope differs')
    mixed, short = directory / 'mixed.fa', directory / 'short.fa'
    mixed.write_bytes(b'>short\nA\n>forward\nACG\n>reverse\nCGT\n')
    short.write_bytes(b'>short\nACG\n>almost\n' + b'A' * 30 + b'\n')
    built, empty = directory / 'built.sig.gz', directory / 'short.sig.gz'
    report = envelope(['sketch', mixed, '--ksize', 3, '--scaled', 1, '--seed', 42,
                       '--name', 'constructed', '-o', built], 'sketch')
    require([report[k] for k in ('records', 'bases', 'windows', 'retained_hashes')] == [3, 7, 2, 1],
            'installed mixed-short construction counts differ')
    with gzip.open(built, 'rt') as stream:
        observed, = json.load(stream)
    require(observed['name'] == 'constructed' and observed['hash_function'] == '0.murmur64' and
            observed['version'] == 0.4 and observed['signatures'][0] ==
            dict(num=0, ksize=3, seed=42, max_hash=18446744073709551615,
                 mins=[1731421407650554201], md5sum='08c05d56c36a44d38e708f93eb4f87b2', molecule='DNA'),
            'installed canonical DNA hash/digest/profile differs from independent one-kmer fixture')
    report = envelope(['sketch', short, '--ksize', 31, '--scaled', 93, '--seed', 42,
                       '--abundance', '--name', 'short', '-o', empty], 'sketch')
    require(report['records'] == 2 and report['bases'] == 33 and report['windows'] == 0 and
            report['retained_hashes'] == 0, 'installed all-short construction differs')
    with gzip.open(empty, 'rt') as stream:
        short_signature, = json.load(stream)
    require(short_signature['signatures'][0]['mins'] == [] and
            short_signature['signatures'][0]['abundances'] == [] and
            short_signature['signatures'][0]['max_hash'] == 198352086814081216,
            'installed scaled93 empty-abundance signature differs')
    collection = [
        literal('a', 18446744073709551615, [1, 9223372036854775809], '397c59a61df2cc1d74a9991f0015b566'),
        literal('b', 18446744073709551615, [1], 'c16a5320fa475530d9583c34fd356ef5'),
        literal('c', 9223372036854775808, [1], 'c16a5320fa475530d9583c34fd356ef5')]
    collection_path, query = directory / 'collection.sig', directory / 'query.sig'
    high, known = directory / 'high.sig', directory / 'known.sig'
    save(collection_path, collection)
    save(query, [collection[0]])
    save(high, [literal('high', 5147545608, [1], 'c16a5320fa475530d9583c34fd356ef5')])
    save(known, [literal('known', 18446744073709551615, [1731421407650554201], '08c05d56c36a44d38e708f93eb4f87b2')])
    inspected = directory / 'inspect.tsv'
    report = envelope(['inspect', built, empty, collection_path, high, '-o', inspected], 'inspect')
    metadata = rows(inspected)
    require(report['signatures'] == 6 and [r['name'] for r in metadata] ==
            ['constructed', 'short', 'a', 'b', 'c', 'high'] and
            [int(r['scaled']) for r in metadata] == [1, 93, 1, 1, 2, 3583599928] and
            metadata[1]['hashes'] == '0' and metadata[1]['abundance'] == 'true',
            'installed gzip reload, labels or scale recovery differs')
    pairs = directory / 'collection-pairs.tsv'
    envelope(['compare', collection_path, '-o', pairs], 'compare')
    pair_rows = rows(pairs)
    require([(r['left'], r['right'], r['scaled']) for r in pair_rows] ==
            [('a', 'b', '2'), ('a', 'c', '2'), ('b', 'c', '2')] and
            all(float(r[key]) == 1 for r in pair_rows for key in
                ('intersection', 'left_hashes', 'right_hashes', 'jaccard', 'left_containment', 'right_containment')),
            'installed collection-wide downsampling differs')
    matrix = directory / 'containment.tsv'
    envelope(['compare', collection_path, '--shape', 'matrix', '--matrix-measure', 'containment', '-o', matrix], 'compare')
    with matrix.open(newline='') as stream:
        matrix_rows = list(csv.reader(stream, delimiter='\t'))
    require(matrix_rows == [['', 'a', 'b', 'c'], ['a', '1', '1', '1'], ['b', '1', '1', '1'],
                            ['c', '1', '1', '1']], 'installed labelled containment matrix differs')
    database = directory / 'database.sig'
    save(database, collection[1:])
    matches = directory / 'search.tsv'
    envelope(['search', query, database, '--threshold', 0, '-o', matches], 'search')
    require([(r['target_filename'], r['score'], r['scaled']) for r in rows(matches)] ==
            [('c.fa', '1', '2'), ('b.fa', '0.5', '1')], 'installed pair-local search/ranking differs')
    connected = directory / 'constructed-pairs.tsv'
    envelope(['compare', built, known, '-o', connected], 'compare')
    compared, = rows(connected)
    require((compared['left'], compared['right'], compared['jaccard']) == ('constructed', 'known', '1'),
            'installed constructed signature did not survive comparison reload')
    connected_search = directory / 'constructed-search.tsv'
    envelope(['search', built, known, '--threshold', 0, '-o', connected_search], 'search')
    found, = rows(connected_search)
    require((found['query_name'], found['target_name'], found['score']) == ('constructed', 'known', '1'),
            'installed constructed signature did not survive search reload')
    invalid = directory / 'invalid.fa'
    invalid.write_bytes(b'>invalid\nACGN\n')
    before = built.read_bytes()
    out, err = run(['--json', 'sketch', invalid, '--ksize', 3, '--check-sequence', '-o', built], status=1)
    diagnostic = json.loads(err.read_bytes())
    require(out.stat().st_size == 0 and diagnostic['status'] == 'error' and diagnostic['error']['kind'] == 'InvalidInput' and
            diagnostic['exit_code'] == 1 and built.read_bytes() == before, 'installed pre-commit failure replaced old output')
    require(len(records) == 11, 'installed lifecycle call count differs')
    save(directory / 'VERIFIED.json', dict(installed_binary=identity(binary), calls=records, short_input_success=True,
         scale93_gzip_roundtrip=True, high_scale_recovery=True, canonical_hash_fixture=True,
         collection_wide_comparison=True, pair_local_search=True, constructed_signature_reloaded=True,
         pre_commit_failure_preserves_output=True, git_foundation_commit=KMER,
         files={p.name: identity(p) for p in directory.iterdir() if p.is_file()},
         scope='installed four-command Git-source workflow only; not local sourmash execution or new performance'))


def bundle(binary):
    temporary = Path(os.environ['RUNNER_TEMP'])
    delivery = temporary / 'delivery'
    binary = Path(binary).resolve(strict=True)
    target = os.environ['NATIVE_TARGET']
    rustc = subprocess.check_output(['rustc', '-vV'], text=True)
    require(HOSTS[target] == os.environ['EXPECTED_HOST'] and '\nhost: ' + HOSTS[target] + '\n' in rustc,
            'bundle is not from its declared native host')
    stem = PRODUCT + '-' + os.environ['PACKAGE_VERSION'] + '-' + target
    folder = delivery / stem
    folder.mkdir()
    shutil.copy2(binary, folder / PRODUCT)
    for name in ('README.md', 'ATTRIBUTION.md', 'LICENSE-MIT', 'LICENSE-APACHE-2.0', 'THIRD_PARTY_NOTICES.md'):
        shutil.copy2(ROOT / name, folder / name)
    shutil.copytree(ROOT / 'licenses', folder / 'licenses')
    shutil.copytree(ROOT / 'benchmarks/2026-08-02', folder / 'benchmarks/2026-08-02')
    (folder / '.github/benchmarks').mkdir(parents=True)
    shutil.copy2(ROOT / '.github/benchmarks/README.md', folder / '.github/benchmarks/README.md')
    provenance = dict(product=PRODUCT, version=os.environ['PACKAGE_VERSION'], target=target,
        head_sha=os.environ['GITHUB_SHA'], run_id=int(os.environ['GITHUB_RUN_ID']),
        run_attempt=int(os.environ['GITHUB_RUN_ATTEMPT']), native_host=HOSTS[target], rustc=rustc,
        machine=subprocess.check_output(['uname', '-a'], text=True).strip(), binary=identity(binary),
        source=json.loads((temporary / 'source-verification.json').read_bytes()),
        native_dependencies=json.loads((delivery / 'native-dependencies.json').read_bytes()),
        installed_lifecycle=json.loads((delivery / 'lifecycle/VERIFIED.json').read_bytes()),
        lock_sha256=identity(ROOT / 'Cargo.lock')['sha256'], git_foundation_commit=KMER,
        prior_corrected_product_base='f430522bb3d3c6fd38e08af758dca11e5262f47b',
        final_head_performance_pending=True, this_binary_remeasured=False, registry_package=False,
        dependencies_vendored=False, signed_release=False)
    require(provenance['source']['source_commit'] == os.environ['GITHUB_SHA'] and
            provenance['installed_lifecycle']['installed_binary'] == identity(binary),
            'bundle does not bind the exact source and exercised installed binary')
    save(folder / 'BUILD.json', provenance)
    paths = [folder, *folder.rglob('*')]
    require(len(paths) == len({str(p.relative_to(folder)).casefold() for p in paths}), 'case-colliding native bundle path')
    for path in paths:
        require(not path.is_symlink() and (path.is_file() or path.is_dir()), 'unsupported native bundle member')
        path.chmod(0o755 if path.is_dir() or path.name == PRODUCT else 0o644)
    archive = delivery / (stem + '.tar.gz')
    with tarfile.open(archive, 'w:gz') as output:
        output.add(folder, arcname=stem)
    source_tar = Path(os.environ['SOURCE_TAR'])
    (delivery / 'SHA256SUMS').write_text(''.join(identity(p)['sha256'] + '  ' + p.name + '\n'
                                              for p in (archive, source_tar)))
    print(json.dumps(provenance, indent=2, sort_keys=True))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('source', 'smoke', 'bundle'))
    parser.add_argument('--binary', type=Path)
    args = parser.parse_args()
    if args.action == 'source':
        source()
    elif args.action == 'smoke':
        smoke(args.binary)
    else:
        bundle(args.binary)


if __name__ == '__main__':
    main()
