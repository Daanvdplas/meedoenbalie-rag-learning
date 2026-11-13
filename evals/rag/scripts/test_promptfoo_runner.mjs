import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
const run=(args)=>spawnSync(process.execPath,['evals/rag/scripts/run_promptfoo.mjs','--live','--suite','models','--ledger','evals/rag/results/local/never-created-test/ledger.json','--max-generations','60','--max-seconds','7200','--binary','/nonexistent-runtime',...args],{encoding:'utf8'});
test('the model launcher rejects an implicit phase before touching runtime or ledger',()=>{
 const result=run([]);assert.notEqual(result.status,0);assert.match(result.stderr,/Models vereist --phase/);
});
test('model smoke requires explicit cases and a single repetition',()=>{
 const result=run(['--phase','smoke','--cases','RAG26-01']);assert.notEqual(result.status,0);assert.match(result.stderr,/caseselectie en --repeat 1/);
});
test('the full protocol cannot silently filter cases or change repetitions',()=>{
 for (const extra of [['--cases','RAG26-01'],['--models','gpt-6.1-sol'],['--repeat','1']]) {
  const result=run(['--phase','pilot',...extra]);assert.notEqual(result.status,0);assert.match(result.stderr,/volledige modelproef behoudt/);
 }
});
