#!/usr/bin/env python3
"""Offline fixture validation used by CI; no model or credentials required."""
import json
from pathlib import Path
from rag_eval import validate_dataset
from eval_integrity import validate_expectations


def main():
    root = Path(__file__).resolve().parents[1] / 'evals/retrieval'
    datasets = {}
    for path in sorted(root.glob('*.json')):
        data = json.loads(path.read_text())
        if 'documents' in data:
            validate_dataset(data)
            datasets[path.stem] = data
            print(f'{path.name}: {len(data["queries"])} queries validated')
    for path in sorted(root.glob('*-answers.json')):
        stem = path.stem.removesuffix('-answers')
        if stem not in datasets:
            raise ValueError(f'No dataset for {path.name}')
        validate_expectations(datasets[stem], json.loads(path.read_text()))
        print(f'{path.name}: answer labels validated')


if __name__ == '__main__':
    main()
