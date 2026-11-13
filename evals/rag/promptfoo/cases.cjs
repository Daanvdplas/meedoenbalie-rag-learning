'use strict';
// Promptfoo owns this case/configuration matrix; no native benchmark/assessor is called here.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { execFileSync } = require('node:child_process');
const ROOT = path.resolve(__dirname, '../../..');
const METHODS = ['dense', 'bm25', 'hybrid_rrf', 'hybrid_weighted'];
const PROFILES = ['documents_sections', 'documents_activities', 'sqlite_sections', 'sqlite_activities'];
const DATASETS = ['legacy', 'independent-20261103', 'expanded-10-20261105', 'strengthened-10-20261106'];
const BASE = { mode: 'local', method: 'dense', profile: 'documents_sections', top_k: 3, candidate_k: 30, context_token_limit: 1200, selection: 'sections', apply_filters: false, mmr: false, rrf_k: 60, vector_weight: 0.7, mmr_lambda: 0.7, model: 'gpt-6.1-sol', filters: {} };
function read(relative) {
  const bytes = fs.readFileSync(path.join(ROOT, relative));
  const pins = JSON.parse(fs.readFileSync(path.join(__dirname, 'data-pins.json'))).files;
  if (pins[relative] !== crypto.createHash('sha256').update(bytes).digest('hex')) throw new Error(`Frozen input changed: ${relative}`);
  return JSON.parse(bytes);
}
function questions(dataset) {
  if (dataset === 'garden') return read('evals/rag/datasets/garden/questions.json').map(row => ({ dataset, case_id: row.id, question: row.text }));
  if (!DATASETS.includes(dataset)) throw new Error('Unknown dataset');
  const base = `evals/shared/datasets/${dataset}/`;
  const inputs = dataset === 'independent-20261103' ? new Map(read(base + 'user-query-inputs.json').cases.map(row => [row.case_id, row.filters])) : null;
  return (dataset === 'legacy' ? ['questions.json', 'heldout-questions.json'] : ['questions.json']).flatMap(name => read(base + name).cases.map(row => ({ dataset, case_id: row.id, question: row.question, ...(inputs ? { filters: inputs.get(row.id) || {} } : {}) })));
}
function configurationName(config) {
  return [config.method, config.profile, config.selection, `k${config.top_k}`, config.apply_filters ? 'filters' : 'unfiltered'].join(':');
}
function testCase(input, config, suite, repeat = 1) {
  const vars = { ...BASE, ...config, ...input };
  return { description: `${suite} ${input.dataset}/${input.case_id} ${configurationName(vars)}`, vars, metadata: { suite, dataset: input.dataset, case_id: input.case_id, configuration: configurationName(vars), reference_scope: input.dataset === 'independent-20261103' ? 'frozen_question_holdout_on_known_five_documents_not_human' : 'existing_development_or_regression_dataset', answer_quality: 'not_assessed' }, options: { repeat, disableVarExpansion: true } };
}
function selected(value) { return value ? new Set(value.split(',').map(part => part.trim()).filter(Boolean)) : null; }
function buildConfig(env = process.env) {
  const suite = env.COA_PROMPTFOO_SUITE || 'retrieval';
  let tests = [];
  if (suite === 'garden') {
    tests = questions('garden').flatMap(input => ['dense', 'keyword_overlap'].map(method => testCase(input, { mode: 'garden', method }, suite)));
  } else if (suite === 'retrieval') {
    tests = DATASETS.flatMap(dataset => questions(dataset).flatMap(input => METHODS.flatMap(method => PROFILES.map(profile => testCase(input, { method, profile }, suite)))));
  } else if (suite === 'quality') {
    const inputs = read('evals/shared/datasets/independent-20261103/user-query-inputs.json').cases;
    const filters = new Map(inputs.map(row => [row.case_id, row.filters]));
    const variants = METHODS.flatMap(method => [ ...PROFILES.map(profile => ({ method, profile })), { method, selection: 'activity_expansion' }, { method, top_k: 6 }, { method, selection: 'activity_expansion', apply_filters: true } ]);
    tests = ['legacy', 'independent-20261103'].flatMap(dataset => questions(dataset).flatMap(input => variants.filter(config => !config.apply_filters || dataset === 'independent-20261103').map(config => testCase(input, { ...config, filters: config.apply_filters && dataset === 'independent-20261103' ? filters.get(input.case_id) || {} : {} }, suite))));
  } else if (suite === 'agentic') {
    const selection = read('evals/rag/datasets/presentation-selection.json').cases;
    tests = selection.flatMap(row => {
      const input = questions(row.dataset_id).find(input => input.case_id === row.case_id);
      if (!input) throw new Error('Frozen agentic selection refers to an unknown question');
      return METHODS.map(method => testCase({ ...input, filters: {} }, { mode: 'codex', method }, suite));
    });
  } else if (suite === 'models') {
    const phase = env.COA_PROMPTFOO_PHASE || 'plan';
    if (!['plan', 'pilot', 'remaining', 'smoke'].includes(phase)) throw new Error('Unknown model protocol phase');
    if (phase === 'smoke' && env.COA_PROMPTFOO_REPEATS !== '1') throw new Error('Model smoke requires explicit repeat1');
    if (['pilot', 'remaining'].includes(phase) && ['COA_PROMPTFOO_CASES', 'COA_PROMPTFOO_MODELS', 'COA_PROMPTFOO_DATASET', 'COA_PROMPTFOO_METHODS', 'COA_PROMPTFOO_PROFILES'].some(key => env[key])) throw new Error('Pilot/remaining use the fixed full model schedule; use smoke for filters');
    let rows = JSON.parse(execFileSync('python3', [path.join(__dirname, 'model_control.py'), 'schedule'], { encoding: 'utf8', env: { ...process.env, ...env } }));
    if (phase === 'pilot') rows = rows.filter(row => row.slot <= 4);
    if (phase === 'remaining') rows = rows.filter(row => row.slot > 4);
    if (phase === 'smoke') rows = rows.filter(row => row.trial === 1);
    const inputs = new Map(questions('strengthened-10-20261106').map(input => [input.case_id, input]));
    tests = rows.map(row => {
      const test = testCase({ ...inputs.get(row.case_id), slot: row.slot, trial: row.trial, phase }, { mode: 'codex', model: row.model }, suite, 1);
      test.description += ` ${row.model} trial${row.trial} slot${row.slot}`;
      test.metadata.model_protocol_phase = phase;
      test.metadata.model_protocol_scope = phase === 'smoke' ? 'technical_smoke_not_the_full_sixty_session_comparison' : 'fixed_interleaved_sixty_session_model_protocol';
      return test;
    });
  } else if (suite === 'compact-live') {
    const settings = [{ case_id: 'T02', method: 'bm25', profile: 'documents_sections' }, { case_id: 'T05', method: 'hybrid_rrf', profile: 'documents_activities' }, { case_id: 'P05', method: 'dense', profile: 'documents_activities' }];
    tests = settings.flatMap((config, index) => (index % 2 ? ['compact', 'full'] : ['full', 'compact']).map(payload_mode => testCase({ ...questions('legacy').find(input => input.case_id === config.case_id), payload_mode }, { ...config, mode: 'compact-live' }, suite)));
  } else if (suite === 'compact') {
    const replay_path = 'evals/rag/fixtures/regressions/t02-dense-search.json';
    const record = read(replay_path);
    const input_sha256 = JSON.parse(fs.readFileSync(path.join(__dirname, 'data-pins.json'))).files[replay_path];
    tests = [testCase({ dataset: 'legacy', case_id: record.case_id, question: record.question, replay_path, input_sha256 }, { ...record.config.retrieval, profile: record.config.profile, mode: 'compact' }, suite)];
    tests[0].metadata.measurement_scope = 'one_existing_canonical_record_not_the_historical_thirteen_call_replay';
  } else throw new Error('Unknown RAG suite');
  for (const [key, variable] of [['COA_PROMPTFOO_DATASET', 'dataset'], ['COA_PROMPTFOO_CASES', 'case_id'], ['COA_PROMPTFOO_METHODS', 'method'], ['COA_PROMPTFOO_PROFILES', 'profile'], ['COA_PROMPTFOO_MODELS', 'model']]) {
    const choices = selected(env[key]);
    if (choices) tests = tests.filter(row => choices.has(row.vars[variable]));
  }
  if (!tests.length) throw new Error('No matching existing tests selected');
  return { description: `COA RAG / ${suite} / existing frozen data`, prompts: ['{{question}}'], providers: [{ id: 'file://provider.py', label: `coa-${suite}-runtime`, config: {} }], tests, defaultTest: { assert: [{ type: 'python', value: suite === 'models' ? 'file://assertions.py:get_model_assert' : 'file://assertions.py:get_assert' }] }, evaluateOptions: { maxConcurrency: 1, maxRetries: 0, cache: false }, commandLineOptions: { cache: false, maxConcurrency: 1, maxRetries: 0 } };
}
module.exports = { buildConfig, questions, configurationName, ROOT };
