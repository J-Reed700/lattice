#!/usr/bin/env python3
"""Offline fixture validation used by CI; no model or credentials required."""
import json
from pathlib import Path
from rag_eval import validate_dataset
from eval_integrity import validate_expectations
from shipped_path_eval import validate_questions
import terse_slice
import course_eval_grading
import tutor_eval


def main():
    root = Path(__file__).resolve().parents[1] / 'evals/retrieval'
    datasets = {}
    for path in sorted(root.glob('*.json')):
        data = json.loads(path.read_text(encoding="utf-8"))
        if 'documents' in data:
            validate_dataset(data)
            datasets[path.stem] = data
            print(f'{path.name}: {len(data["queries"])} queries validated')
    for path in sorted(root.glob('*-answers.json')):
        stem = path.stem.removesuffix('-answers')
        if stem not in datasets:
            raise ValueError(f'No dataset for {path.name}')
        validate_expectations(datasets[stem], json.loads(path.read_text(encoding="utf-8")))
        print(f'{path.name}: answer labels validated')
    # Desktop-path question sets name their corpus; every rubric fact must
    # occur in the documents it expects to be cited.
    for path in sorted(root.glob('desktop-path-*.json')):
        questions = json.loads(path.read_text(encoding="utf-8"))
        corpus = json.loads((root.parents[1] / questions['corpus']).read_text(encoding="utf-8"))
        problems = validate_questions(questions, corpus)
        if problems:
            raise ValueError(f'{path.name}: ' + '; '.join(problems))
        print(f'{path.name}: {len(questions["items"])} desktop-path items validated')
    # Terse slices restate a source fixture's queries; each must cover every
    # eligible source query once, in new words.
    for path in sorted(root.glob('*-terse.json')):
        report = terse_slice.check(*terse_slice.load(path))
        if report['problems']:
            raise ValueError(f'{path.name}: ' + '; '.join(report['problems']))
        print(f'{path.name}: {report["queries"]} terse queries validated')
    # The course eval's quiz grading sets: rows, bank, recomputed expected grades and frozen hashes.
    for name, problems in course_eval_grading.validate().items():
        if problems:
            raise ValueError(f'{name}: ' + '; '.join(problems))
        print(f'{name}: quiz grading set validated')
    # The tutor eval's synthetic course and its scripted sessions.
    tutor = root.parent / 'tutor'
    course = json.loads((tutor / 'course-v1.json').read_text(encoding='utf-8'))
    sessions = json.loads((tutor / 'sessions-v1.json').read_text(encoding='utf-8'))
    problems = tutor_eval.validate_course(course) + tutor_eval.validate_sessions(sessions, course)
    if problems:
        raise ValueError('evals/tutor: ' + '; '.join(problems))
    print(f'evals/tutor: {len(sessions["sessions"])} sessions validated')


if __name__ == '__main__':
    main()
