"""Manual inspection of strict sanitized receipts, never free-text log inference."""
import local_toolkit_evidence as evidence
import local_review_batch as local

MAX_EVENTS = 256
KINDS = {'empty_response', 'response_limit', 'content_filter', 'explicit_refusal',
         'context_limit', 'provider_pause', 'unknown_stop_reason', 'duplicate_tool_call_identity'}


def validate(doc):
    if (not isinstance(doc, dict) or set(doc) != {'schema_version', 'sanitized', 'provenance', 'complete', 'events'}
            or type(doc['schema_version']) is not int or doc['schema_version'] != 1
            or doc['sanitized'] is not True or type(doc['complete']) is not bool
            or doc['provenance'] not in ('synthetic', 'historical', 'unknown')
            or not isinstance(doc['events'], list) or len(doc['events']) > MAX_EVENTS):
        raise ValueError('invalid receipt')
    for event in doc['events']:
        if not isinstance(event, dict):
            raise ValueError('invalid event')
        kind = event.get('type')
        if kind == 'recall_returned':
            if (set(event) != {'type', 'message_index', 'offset', 'count', 'reran_tool'}
                    or event['reran_tool'] is not False
                    or any(type(event[k]) is not int or not 0 <= event[k] <= 1000000000
                           for k in ('message_index', 'offset', 'count'))
                    or not 1 <= event['count'] <= 12000):
                raise ValueError('invalid recall event')
        elif kind == 'path_rejected':
            if set(event) != {'type', 'reason'} or event['reason'] not in ('outside_workspace', 'symlink_escape', 'invalid_path', 'unknown'):
                raise ValueError('invalid path event')
        elif kind == 'provider_stop':
            if set(event) != {'type', 'kind'} or event['kind'] not in tuple(KINDS):
                raise ValueError('invalid stop event')
        else:
            raise ValueError('unknown event')


def diagnose(root, files):
    root = local.selection(root, files)
    rows = []
    for ordinal, path in enumerate(files, 1):
        row = {'input_number': ordinal, 'state': 'malformed_or_unreadable', 'execution_verified': False}
        try:
            doc, digest = evidence.load_json(root, path)
            validate(doc)  # Entire receipt checked before any classification.
            counts = {'recall_returned': 0, 'duplicate_recall_observation': 0, 'path_rejected': 0,
                      'provider_empty_stop': 0, 'provider_incomplete_stop': 0, 'duplicate_tool_call_identity': 0}
            seen = set()
            for event in doc['events']:
                if event['type'] == 'recall_returned':
                    counts['recall_returned'] += 1
                    key = tuple(event[k] for k in ('message_index', 'offset', 'count'))
                    if key in seen:
                        counts['duplicate_recall_observation'] += 1
                    seen.add(key)
                elif event['type'] == 'path_rejected':
                    counts['path_rejected'] += 1
                else:
                    key = ('provider_empty_stop' if event['kind'] == 'empty_response' else
                           'duplicate_tool_call_identity' if event['kind'] == 'duplicate_tool_call_identity'
                           else 'provider_incomplete_stop')
                    counts[key] += 1
            row.update({'state': 'reviewed', 'sha256': digest, 'provenance': doc['provenance'],
                        'declared_complete': doc['complete'], 'counts': counts,
                        'evidence_incomplete': not doc['complete'] or not doc['events'] or doc['provenance'] == 'unknown'})
        except (OSError, ValueError, RecursionError, TypeError):
            # No paths, exception strings, transcript excerpts, IDs or unknown values are echoed.
            row['diagnostic'] = 'Input unreadable or not a supported sanitized receipt; no classifications accepted'
        rows.append(row)
    complete = all(r['state'] == 'reviewed' and not r['evidence_incomplete'] for r in rows)
    return {'schema_version': 1, 'ok': complete, 'coverage_complete': complete, 'inputs': rows,
            'execution_verified': False, 'causality_established': False,
            'scope': 'Explicit structured receipt assertions only; no free-text inference or replay',
            'duplicate_meaning': 'Repeated selected recall coordinates within one receipt, not proof of repeated execution'}
