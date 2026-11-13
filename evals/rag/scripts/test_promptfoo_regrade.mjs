import test from 'node:test';
import assert from 'node:assert/strict';
import { buildRegradeConfig } from './regrade_promptfoo_results.mjs';

test('regrading uses only recorded outputs and keeps the original execution binding',()=>{
 const output={schema_version:1,status:'completed',operation:'codex',record:{live:{run_id:'native-original',status:'completed'}}};
 const raw={evalId:'eval-original',results:{timestamp:'2026-11-13T18:00:00Z',results:[{id:'row-original',provider:{label:'original'},testCase:{vars:{dataset:'legacy',case_id:'P01',question:'Wanneer?',mode:'codex',model:'gpt-6.1-sol',expected:'DO NOT LEAK'}},response:{output,metadata:{execution_mode:'fresh_runtime_execution'}}}]}};
 const config=buildRegradeConfig(raw,'abc123');
 assert.deepEqual(config.providers,['echo']);
 assert.equal(config.evaluateOptions.maxConcurrency,1);
 assert.deepEqual(JSON.parse(config.tests[0].vars.recorded_output),output);
 assert.equal(config.tests[0].metadata.original_evaluation_id,'eval-original');
 assert.equal(config.tests[0].metadata.original_result_id,'row-original');
 assert.equal(config.tests[0].metadata.original_raw_sha256,'abc123');
 assert.equal(config.tests[0].metadata.execution_origin,'recorded_execution_regrading');
 assert.equal(config.tests[0].vars.expected,undefined);
 assert.equal(config.tests[0].options.repeat,1);
});

test('regrading refuses missing provider outputs rather than creating empty evidence',()=>{
 assert.throws(()=>buildRegradeConfig({evalId:'eval',results:{results:[{testCase:{vars:{}}}]}},'hash'),/recorded|output/i);
});
