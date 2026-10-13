#!/usr/bin/env python3
"""Offline fixture validation used by CI; no model or credentials required."""
import json
from pathlib import Path
from rag_eval import validate_dataset
from eval_integrity import validate_expectations
from shipped_path_eval import validate_questions


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
    # Desktop-path question sets name their corpus; every rubric fact must
    # occur in the documents it expects to be cited.
    for path in sorted(root.glob('desktop-path-*.json')):
        questions = json.loads(path.read_text())
        corpus = json.loads((root.parents[1] / questions['corpus']).read_text())
        problems = validate_questions(questions, corpus)
        if problems:
            raise ValueError(f'{path.name}: ' + '; '.join(problems))
        print(f'{path.name}: {len(questions["items"])} desktop-path items validated')


if __name__ == '__main__':
    main()
