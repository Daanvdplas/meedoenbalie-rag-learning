import importlib.util
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name('export_site_results.py')


class PublicResultsExportTests(unittest.TestCase):
    def load_exporter(self):
        self.assertTrue(SCRIPT.exists(), 'The public-safe results exporter is missing')
        spec = importlib.util.spec_from_file_location('site_exporter', SCRIPT)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module

    def sample(self):
        section = dict(source_id='ACT-TEST', id='S1', source_title='Test', source_date='2026-11-05', heading='Wanneer?', text='Dinsdag om tien uur.')
        native = {'answer': {'text': 'Dinsdag.', 'reasoning': 'DO-NOT-EXPORT'}, 'tool_calls': [{'query': 'Wanneer?', 'status': 'completed', 'result': {'hits': [{'section': section}], 'private_path': '/Users/PRIVATE'}, 'arguments': {'auth_token': 'SECRET'}}], 'reasoning': 'HIDDEN', 'runtime_path': '/private/tmp/PRIVATE'}
        ref = {'source_id': 'ACT-TEST', 'section_id': 'S1'}
        assessment = {'actual_search_calls': 1, 'source_coverage': {'complete': True, 'first_complete_call_index': 1, 'expected_source_options': [[ref]]}, 'calls': [{'index': 1, 'query': 'Wanneer?', 'status': 'completed', 'offered_sections': [ref], 'complete_expected_sources': True}]}
        return native, assessment, {'ACT-TEST:S1': section}

    def test_projection_keeps_observables_without_private_native_fields(self):
        module = self.load_exporter()
        native, assessment, sources = self.sample()
        projected = module.project_run(native, assessment, sources)
        self.assertEqual(projected['calls'][0]['hits'], ['ACT-TEST:S1'])
        self.assertEqual(projected['calls'][0]['query'], 'Wanneer?')
        self.assertTrue(projected['complete'])
        import json
        exported = json.dumps(projected)
        for forbidden in ['reasoning', 'runtime_path', 'arguments', 'auth_token', 'PRIVATE', 'SECRET', 'HIDDEN']:
            self.assertNotIn(forbidden, exported)

    def test_projection_rejects_unoffered_or_changed_source_text(self):
        module = self.load_exporter()
        native, assessment, sources = self.sample()
        native['tool_calls'][0]['result']['hits'][0]['section']['text'] = 'Tampered source.'
        sources['ACT-TEST:S1'] = dict(sources['ACT-TEST:S1'], text='Dinsdag om tien uur.')
        with self.assertRaises(ValueError):
            module.project_run(native, assessment, sources)

    def test_full_alternative_cannot_be_built_from_two_different_options(self):
        module = self.load_exporter()
        self.assertFalse(module.is_complete({'A:S2', 'B:S3'}, [['A:S2', 'A:S3'], ['B:S2', 'B:S3']]))
        self.assertTrue(module.is_complete({'B:S2', 'B:S3'}, [['A:S2', 'A:S3'], ['B:S2', 'B:S3']]))


if __name__ == '__main__':
    unittest.main()
