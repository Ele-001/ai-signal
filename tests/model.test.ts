import {test} from 'node:test';
import assert from 'node:assert/strict';
import {aggregate,mergeTask,restore,validTask,statuses} from '../src/model.ts';
import type {Task} from '../src/model.ts';
const task=(status:Task['status'],updatedAt=1):Task=>({id:status,title:'task',status,source:'test',updatedAt});
test('aggregation always chooses highest urgency regardless of ordering',()=>{
  assert.equal(aggregate([]),'idle');
  for(let i=0;i<statuses.length;i++) assert.equal(aggregate(statuses.slice(i).reverse().map(s=>task(s))),statuses[i]);
  assert.equal(aggregate([task('completed'),task('running')]),'running');
});
test('corrupt saved state is contained, numeric preferences clamp, task data validates',()=>{
  assert.equal(restore('broken').tasks.length,0);
  const result=restore(JSON.stringify({version:1,tasks:[task('attention'),{}, {...task('running'),status:'unknown'}],settings:{scale:99,opacity:0,sound:'yes'}}));
  assert.equal(result.tasks.length,1);assert.equal(result.settings.scale,1.6);assert.equal(result.settings.opacity,.4);assert.equal(result.settings.sound,false);
  assert.equal(validTask({...task('running'),updatedAt:NaN}),false);
});
test('signal styles persist and old or unknown settings use the classic default',()=>{
  assert.equal(restore(null).settings.style,'classic');
  assert.equal(restore(JSON.stringify({version:1,settings:{style:'playful'}})).settings.style,'playful');
  assert.equal(restore(JSON.stringify({version:1,settings:{style:'unknown'}})).settings.style,'classic');
  assert.equal(restore(JSON.stringify({version:1,settings:{alwaysOnTop:false}})).settings.alwaysOnTop,false);
  assert.equal(restore(null).settings.motion,true);
  assert.equal(restore(JSON.stringify({version:1,settings:{motion:false}})).settings.motion,false);
  assert.equal(restore(JSON.stringify({version:1,settings:{motion:'false'}})).settings.motion,true);
});
test('older events cannot overwrite newer task state; event snapshots merge by id',()=>{
  const newer=task('running',20);
  assert.deepEqual(mergeTask([newer],{...newer,status:'completed',updatedAt:10}),[newer]);
  assert.equal(mergeTask([newer],{...newer,status:'completed',updatedAt:21})[0].status,'completed');
});
test('all 512 native tasks survive hydration, including an older urgent task',()=>{
  let tasks:Task[]=[];
  for(let i=0;i<512;i++) tasks=mergeTask(tasks,{...task(i===0?'attention':'completed',i),id:String(i)});
  assert.equal(tasks.length,512);assert.equal(aggregate(tasks),'attention');
  assert.equal(validTask({...task('running'),title:'🟢'.repeat(200)}),true);
  assert.equal(validTask({...task('running'),title:'🟢'.repeat(201)}),false);
});
