"""Dependency-free integrity primitives shared by evaluation runners and gates."""
import hashlib
import json
import math
import platform
import subprocess
from pathlib import Path

SCHEMA_VERSION = 2


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, ensure_ascii=False,
                                     separators=(',', ':'), allow_nan=False).encode()).hexdigest()


def unique_records(records, key):
    result = {}
    for row in records:
        if not isinstance(row, dict) or not isinstance(row.get(key), str) or not row[key].strip():
            raise ValueError(f'Every record needs a nonempty {key}')
        if row[key] in result:
            raise ValueError(f'Duplicate {key}: {row[key]}')
        result[row[key]] = row
    return result


def finite_number(value, name, minimum=0):
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value < minimum:
        raise ValueError(f'{name} must be finite and >= {minimum}')
    return value


def validate_expectations(dataset, data):
    rows = unique_records(data['queries'], 'id')
    if set(rows) != {q['id'] for q in dataset['queries']}:
        raise ValueError('Expectations must cover exactly the dataset query IDs')
    import re
    for query in dataset['queries']:
        row = rows[query['id']]
        relevant = {doc for doc, grade in query['relevance'].items() if grade > 0}
        abstain = row.get('expected_abstain', not relevant)
        if type(abstain) is not bool or abstain != (not relevant):
            raise ValueError(f"{query['id']}: abstention label conflicts with relevance")
        groups = row.get('required_patterns', [])
        if not isinstance(groups, list) or (not abstain and not groups):
            raise ValueError(f"{query['id']}: answerable expectations need fact patterns")
        forbidden = row.get('forbidden_patterns', [])
        if not isinstance(forbidden, list):
            raise ValueError('forbidden_patterns must be a list')
        for index, group in enumerate(groups + [forbidden]):
            if not isinstance(group, list) or (index < len(groups) and not group):
                raise ValueError('Pattern groups must be nonempty lists')
            for pattern in group:
                if not isinstance(pattern, str) or not pattern:
                    raise ValueError('Patterns must be nonempty strings')
                re.compile(pattern)
        required = row.get('required_citation_docs', list(relevant))
        if not isinstance(required, list) or len(required) != len(set(required)) or not set(required).issubset(relevant):
            raise ValueError('Required citations must be unique relevant document IDs')
    return rows


def manifest(dataset, expectations, retrieval, settings):
    try:
        commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True, stderr=subprocess.DEVNULL).strip()
    except (OSError, subprocess.CalledProcessError):
        commit = None
    scripts = Path(__file__).parent
    identity = {
        'schema_version': SCHEMA_VERSION,
        'dataset_sha256': digest(dataset),
        'expectations_sha256': digest(expectations),
        'retrieval_sha256': digest(retrieval),
        'settings': settings,
        'harness_sha256': digest({p.name: p.read_text() for p in
                                 [scripts / 'chat_rag_eval.py', scripts / 'rag_eval.py', Path(__file__)]}),
    }
    return {**identity, 'run_id': digest(identity), 'commit': commit,
            'environment': {'python': platform.python_version(), 'platform': platform.platform()}}


def prepare_output(output, current, resume):
    """Refuse accidental overwrite, partial metadata, or incompatible continuation."""
    output = Path(output)
    sidecar = output.with_suffix(output.suffix + '.manifest.json')
    existing = []
    if output.exists() or sidecar.exists():
        if not resume:
            raise ValueError('Output already exists; use a new path or --resume')
        if not sidecar.exists() or json.loads(sidecar.read_text()).get('run_id') != current['run_id']:
            raise ValueError('Resume manifest is missing or incompatible')
        if output.exists():
            existing = [json.loads(line) for line in output.read_text().splitlines() if line.strip()]
        unique_records(existing, 'query_id')
        if any(row.get('run_id') != current['run_id'] for row in existing):
            raise ValueError('Resume rows do not match the run manifest')
    else:
        output.parent.mkdir(parents=True, exist_ok=True)
        sidecar.write_text(json.dumps(current, indent=2, allow_nan=False) + '\n')
    return existing


def wilson(successes, count):
    """95% Wilson interval; never fabricate precision for an empty sample."""
    if not count:
        return None
    z = 1.959963984540054
    p = successes / count
    denominator = 1 + z*z/count
    center = (p + z*z/(2*count))/denominator
    half = z*math.sqrt(p*(1-p)/count + z*z/(4*count*count))/denominator
    return [max(0, center-half), min(1, center+half)]
