import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createElement} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import {Signal} from '../src/Signal.tsx';
import {statuses} from '../src/model.ts';

test('both lamp designs illuminate exactly one correct lamp and leave idle unlit',()=>{
  for(const style of ['classic','playful'] as const) {
    for(const status of statuses) {
      const markup=renderToStaticMarkup(createElement(Signal,{status,style}));
      assert.equal((markup.match(/lamp-active/g)??[]).length,status==='idle'?0:1);
      assert.ok(markup.includes(`signal-${status} signal-motion`));
      assert.equal(markup.includes('class="attention-ring"'),status==='attention');
    }
  }
});
test('static mode is explicit in both designs without losing the active status',()=>{
  for(const style of ['classic','playful'] as const) {
    const markup=renderToStaticMarkup(createElement(Signal,{status:'running',style,motion:false}));
    assert.ok(markup.includes('signal-running signal-static'));
    assert.ok(!markup.includes('signal-motion'));
    assert.equal((markup.match(/lamp-active/g)??[]).length,1);
  }
});
