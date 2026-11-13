const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const configPath = path.resolve(__dirname, '../cases.cjs');
function build(suite, filters = {}) {
  assert.ok(fs.existsSync(configPath), 'Canonical promptfoo testcase/config matrix is not implemented');
  return require(configPath).buildConfig({ COA_PROMPTFOO_SUITE: suite, ...filters });
}
test('retrieval matrix owns all fifty existing questions and sixteen configurations', () => {
  const config = build('retrieval');
  assert.equal(config.tests.length, 800);
  assert.equal(new Set(config.tests.map(row => `${row.vars.dataset}:${row.vars.case_id}`)).size, 50);
  assert.equal(new Set(config.tests.map(row => `${row.vars.method}:${row.vars.profile}`)).size, 16);
  for (const row of config.tests) for (const forbidden of ['expected', 'facts', 'evidence_alternatives', 'relevant_sections', 'forbidden_claims']) assert.equal(forbidden in row.vars, false);
  assert.equal(config.evaluateOptions.maxConcurrency, 1);
  assert.equal(config.evaluateOptions.maxRetries, 0);
});
test('garden keeps the real five questions and the historical Jaccard baseline', () => {
  const config = build('garden');
  assert.equal(config.tests.length, 10);
  assert.deepEqual([...new Set(config.tests.map(row => row.vars.method))].sort(), ['dense', 'keyword_overlap']);
});
test('quality preserves the historical 24 legacy and 28 independent configuration sets', () => {
  const config = build('quality');
  assert.equal(config.tests.length, 768);
  assert.equal(config.tests.filter(row => row.vars.dataset === 'legacy').length,432);
  assert.equal(config.tests.filter(row => row.vars.dataset === 'independent-20261103').length,336);
  assert.equal(new Set(config.tests.filter(row => row.vars.dataset === 'legacy').map(row => row.metadata.configuration)).size,24);
  assert.equal(new Set(config.tests.map(row => row.metadata.configuration)).size, 28);
  const filtered = config.tests.filter(row => row.vars.apply_filters);
  assert.ok(filtered.some(row => row.vars.dataset === 'independent-20261103' && Object.keys(row.vars.filters).length));
  assert.ok(filtered.filter(row => row.vars.dataset === 'legacy').every(row => !Object.keys(row.vars.filters).length));
});
test('agentic and repeated model trials preserve their real question selections', () => {
  assert.equal(build('agentic').tests.length, 40);
  const models = build('models');
  assert.equal(models.tests.length, 60);
  assert.ok(models.tests.every(row => row.options.repeat === 1 && row.vars.method === 'dense'));
  assert.deepEqual([...new Set(models.tests.map(row => row.vars.model))].sort(), ['gpt-6-luna', 'gpt-6.1-sol']);
  assert.ok(models.providers.every(provider => provider.id.startsWith('file://')));
});
test('filters select real existing tests instead of inventing a dataset or case', () => {
  const config = build('retrieval', { COA_PROMPTFOO_DATASET: 'legacy', COA_PROMPTFOO_CASES: 'T05,P05', COA_PROMPTFOO_METHODS: 'bm25', COA_PROMPTFOO_PROFILES: 'documents_sections' });
  assert.equal(config.tests.length, 2);
  assert.throws(() => build('retrieval', { COA_PROMPTFOO_CASES: 'NOT_A_CASE' }), /selected|matching/i);
});
test('quality configuration signatures equal the retained native preregistration', { skip: !process.env.COA_RAG_BINARY }, () => {
  const { spawnSync } = require('node:child_process');
  const config = build('quality');
  const signature = (profile, c) => JSON.stringify([profile, c.method, c.selection, c.top_k, c.candidate_k, c.context_token_limit, c.apply_filters, c.mmr, c.rrf_k, c.vector_weight, c.mmr_lambda]);
  for (const dataset of ['legacy', 'independent-20261103']) {
    const native = spawnSync(process.env.COA_RAG_BINARY, ['lab', 'plan', '--dataset', dataset, '--protocol', 'quality-v1'], { encoding: 'utf8' });
    assert.equal(native.status, 0, native.stderr);
    const expected = new Set(JSON.parse(native.stdout).configurations.map(row => signature(row.config.profile, row.config.retrieval)));
    const actual = new Set(config.tests.filter(row => row.vars.dataset === dataset).map(row => signature(row.vars.profile, row.vars)));
    assert.deepEqual([...actual].sort(), [...expected].sort());
  }
});
test('frozen independent user filters stay input context even when filtering is off; agentic keeps its old unfiltered treatment', () => {
  const matrix = build('retrieval', { COA_PROMPTFOO_DATASET: 'independent-20261103', COA_PROMPTFOO_CASES: 'IND26-01', COA_PROMPTFOO_METHODS: 'bm25', COA_PROMPTFOO_PROFILES: 'documents_sections' });
  assert.equal(matrix.tests[0].vars.apply_filters, false);
  assert.deepEqual(matrix.tests[0].vars.filters, { environment: 'indoor' });
  assert.ok(build('agentic').tests.every(row => Object.keys(row.vars.filters).length === 0));
});
test('explicit model smoke override selects Sol and Luna exactly once while default remains three repetitions', () => {
  const config = build('models', { COA_PROMPTFOO_CASES: 'RAG26-01', COA_PROMPTFOO_REPEATS: '1', COA_PROMPTFOO_PHASE: 'smoke' });
  assert.equal(config.tests.length, 2);
  assert.ok(config.tests.every(row => row.options.repeat === 1));
  assert.deepEqual(config.tests.map(row => row.vars.model).sort(), ['gpt-6-luna', 'gpt-6.1-sol']);
  assert.equal(build('models').tests.reduce((count, row) => count + row.options.repeat, 0), 60);
});

test('model phases keep the exact historical interleaving and four-pilot boundary', () => {
  const { execFileSync } = require('node:child_process');
  const expected = JSON.parse(execFileSync('python3', ['-c', "import importlib.util,json;from pathlib import Path;p=Path('evals/rag/scripts/model_trials.py');s=importlib.util.spec_from_file_location('oracle',p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);print(json.dumps(m.schedule([f'RAG26-{i:02}' for i in range(1,11)])))"], { cwd: require(configPath).ROOT, encoding: 'utf8' }));
  const actual = build('models').tests.map(row => ({ slot: row.vars.slot, model: row.vars.model, case_id: row.vars.case_id, trial: row.vars.trial }));
  assert.deepEqual(actual, expected);
  const directory=fs.mkdtempSync(path.join(require('node:os').tmpdir(),'coa-model-schedule-'));
  try {
    const env={COA_PROMPTFOO_LEDGER:path.join(directory,'ledger.json')};
    const pilot=build('models',{...env,COA_PROMPTFOO_PHASE:'pilot'});
    assert.equal(pilot.tests.length,4);
    assert.deepEqual(pilot.tests.map(row=>row.vars.slot),[1,2,3,4]);
    assert.throws(()=>build('models',{...env,COA_PROMPTFOO_PHASE:'remaining'}),/pilot|approval|Command failed/);
  } finally {fs.rmSync(directory,{recursive:true,force:true});}
  assert.throws(()=>build('models',{COA_PROMPTFOO_PHASE:'pilot',COA_PROMPTFOO_CASES:'RAG26-01'}),/smoke|fixed|frozen/i);
});
