# Trusted regression fixtures, independently authored from the lesson generator.
# Reference: https://docs.python.org/3.14/library/csv.html (accessed 2026-10-04).
# These observations establish behavior for these inputs, not universal claims.
import csv
import io
import json

cases = [
    ('header_whitespace', ' category ,count\nfood,2\n', {}),
    ('skipinitialspace', ' category , count \nfood,2\n', {'skipinitialspace': True}),
    ('empty_input', '', {}),
    ('newline_only', '\n', {}),
    ('missing_field', 'category,count\nfood\n', {}),
    ('blank_data_line', 'category,count\n\nfood,2\n', {}),
]
observed = {}
for name, text, options in cases:
    reader = csv.DictReader(io.StringIO(text), **options)
    rows = list(reader)
    observed[name] = {'input': text, 'options': options, 'fieldnames': reader.fieldnames, 'rows': rows}

assert observed['header_whitespace']['fieldnames'] == [' category ', 'count']
assert observed['skipinitialspace']['fieldnames'] == ['category ', 'count ']
assert observed['empty_input']['fieldnames'] is None
assert observed['newline_only']['fieldnames'] == []
assert observed['missing_field']['rows'] == [{'category': 'food', 'count': None}]
assert observed['blank_data_line']['rows'] == [{'category': 'food', 'count': '2'}]
try:
    '2' + 1
except TypeError:
    observed['string_plus_integer'] = {'expression': "'2' + 1", 'exception': 'TypeError'}
else:
    raise AssertionError('string-plus-integer unexpectedly succeeded')
print(json.dumps(observed, ensure_ascii=False, sort_keys=True))
