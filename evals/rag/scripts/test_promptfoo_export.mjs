import test from 'node:test';
import assert from 'node:assert/strict';
import { project } from './export_promptfoo_results.mjs';
const section = {source_id:'ACT-X',source_date:'2026-11-13',source_title:'Test',id:'S2',heading:'Wanneer?',text:'Iedere dinsdag.'};
function fixture(metrics) {
 return {evalId:'eval-test',results:{timestamp:'2026-11-13T12:00:00Z',results:[{
  id:'result-1',provider:{id:'file://private-provider.py',label:'Lokale retrieval'},
  testCase:{vars:{dataset:'legacy',case_id:'T05',question:'Wanneer?',method:'bm25',profile:'documents_sections',suite:'retrieval',mode:'local',model:'gpt-6.1-sol'}},
  response:{output:{status:'completed',record:{status:'completed',run_id:'native-1',outcome:{hits:[{section}],candidates:[{secret:'PRIVATE'}]},audit_trace:{path:'/Users/PRIVATE'}}},metadata:{secret:'PRIVATE'}},
  success:false,latencyMs:3,gradingResult:{pass:false,reason:'Bronnen ontbreken',componentResults:[{pass:false,score:0.5,reason:'1 van 2 bewijsdoelen',metadata:{source_metrics:metrics}}]},private:'PRIVATE'
 }]}};
}
test('actual source failures remain failures and public output only includes allowed evidence',()=>{
 const result=project(fixture({target_goals:2,covered_goals:1,source_quality_applicable:true}));
 assert.equal(result.evaluations[0].rows[0].status,'failed');
 assert.equal(result.evaluations[0].source_completion.denominator,1);
 assert.equal(result.evaluations[0].source_completion.numerator,0);
 assert.equal(Object.values(result.sources)[0].text,'Iedere dinsdag.');
 assert.ok(!JSON.stringify(result).includes('PRIVATE'));
 assert.equal(result.evaluations[0].rows[0].native_run_id,'native-1');
});
test('targetless questions do not inflate the source-quality denominator',()=>{
 const result=project(fixture({target_goals:0,covered_goals:0,source_quality_applicable:false}));
 assert.equal(result.evaluations[0].source_completion.denominator,0);
 assert.equal(result.evaluations[0].rows[0].source_complete,null);
});
test('incomplete provider execution cannot pass even when partial hits look complete',()=>{
 const input=fixture({target_goals:2,covered_goals:2,source_quality_applicable:true});
 const row=input.results.results[0];row.response.output.status='timed_out';row.gradingResult.pass=true;
 const result=project(input);
 assert.equal(result.evaluations[0].rows[0].status,'error');
 assert.equal(result.evaluations[0].rows[0].source_complete,null);
 assert.equal(result.evaluations[0].source_completion.denominator,0);
});

test('local retrieval does not present a configured Codex default as a called model',()=>{
 const result=project(fixture({target_goals:0,covered_goals:0,source_quality_applicable:false}));
 assert.equal(result.evaluations[0].rows[0].configuration.model,undefined);
});

test('a rejected evidence binding is an execution error without source-quality credit',()=>{
 const input=fixture({target_goals:2,covered_goals:2,source_quality_applicable:true});
 const grade=input.results.results[0].gradingResult;
 grade.pass=false;grade.componentResults=[{pass:false,score:0,reason:'Evidence binding failed',namedScores:{execution_integrity:0}}];
 const row=project(input).evaluations[0].rows[0];
 assert.equal(row.status,'error');
 assert.equal(row.source_complete,null);
});

test('compact replay retains inspectable source passages without exporting full native payloads',()=>{
 const input=fixture({target_goals:0,source_quality_applicable:false});
 input.results.results[0].response.output.record={status:'completed',execution_mode:'offline_projection_replay',full_response:{question:'Wanneer?',hits:[{section}],private:'PRIVATE'},compact_response_json:'PRIVATE'};
 const result=project(input);
 assert.equal(result.evaluations[0].rows[0].calls[0]?.hits.length,1);
 assert.equal(Object.values(result.sources)[0]?.text,section.text);
 assert.ok(!JSON.stringify(result).includes('PRIVATE'));
});

test('offline regrading is labeled separately from the original native execution',()=>{
 const input=fixture({target_goals:1,covered_goals:1,source_quality_applicable:true});
 input.results.results[0].testCase.metadata={execution_origin:'recorded_execution_regrading',original_evaluation_id:'eval-native',original_timestamp:'2026-11-13T18:00:00Z'};
 const row=project(input).evaluations[0].rows[0];
 assert.equal(row.execution_origin,'recorded_execution_regrading');
 assert.equal(row.original_evaluation_id,'eval-native');
 assert.equal(row.original_timestamp,'2026-11-13T18:00:00Z');
 assert.equal(row.native_run_id,'native-1');
});

test('only independently verified model failures count as failed outcomes without source credit',()=>{
 const input=fixture({execution_integrity:true,model_failure_class:'model_output_invalid',target_goals:0,source_quality_applicable:false});
 input.results.results[0].response.output.record.status='failed';
 const row=project(input).evaluations[0].rows[0];
 assert.equal(row.status,'failed');assert.equal(row.source_complete,null);
 input.results.results[0].gradingResult.componentResults[0].metadata.source_metrics.execution_integrity=false;
 assert.equal(project(input).evaluations[0].rows[0].status,'error');
});
