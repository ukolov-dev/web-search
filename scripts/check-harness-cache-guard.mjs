import assert from 'node:assert/strict'
import { mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import SearchOnly from './harness-cache-guard.mjs'

const directory=mkdtempSync(join(tmpdir(),'harness-guard-'))
const allowed=['search','ranked_search','discover','extract'].map(n=>`mcp__web_search__${n}`)
let guard, dispose, section
const tools={view:()=>({visible:new Map([...allowed,'bash','web_search'].map(n=>[n,{}]))}),guard:fn=>{guard=fn;return ()=>{guard=undefined}}}
const original=tools.view
new SearchOnly({tools,systemPrompt:{section:x=>{section=x}},on:(_,fn)=>{dispose=fn}},{ready:join(directory,'ready.json')})
try {
  assert.deepEqual([...tools.view({}).visible.keys()],allowed)
  assert.equal(guard({name:'bash'}),'BENCHMARK_SEARCH_TOOLS_ONLY')
  assert.equal(guard({name:allowed[0],arguments:{engine:'ddg'}}),'BENCHMARK_SOURCE_NOT_QUALIFIED')
  const agent={}
  for(let i=0;i<4;i++) assert.equal(guard({name:allowed[0],arguments:{engine:'seznam'},agent}),undefined)
  assert.equal(guard({name:allowed[3],agent}),'BENCHMARK_TOOL_BUDGET')
  assert.equal(section.complete,true)
  await new Promise(resolve=>setTimeout(resolve,150))
  assert.deepEqual(JSON.parse(readFileSync(join(directory,'ready.json'),'utf8')),allowed)
} finally { dispose();rmSync(directory,{recursive:true,force:true}) }
assert.equal(tools.view,original)
console.log('Harness guard: native four-tool view, source filter, budget and readiness passed')
