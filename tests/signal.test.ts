import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createElement} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import {Signal} from '../src/Signal.tsx';
import {statuses} from '../src/model.ts';

test('all three vertical lamp designs illuminate exactly one correct lamp and leave idle unlit',()=>{
  for(const style of ['classic','silver','mechanical'] as const) {
    for(const status of statuses) {
      const markup=renderToStaticMarkup(createElement(Signal,{status,style}));
      assert.ok(markup.includes('viewBox="0 0 150 292"'));
      assert.equal((markup.match(/lamp-active/g)??[]).length,status==='idle'?0:1);
      if(status!=='idle') {
        const index=status==='attention'?0:status==='repairing'?1:2;
        assert.equal(Number(markup.match(/class="lamp lamp-active"><circle cx="75" cy="(\d+)"/)?.[1]),60+index*(style==='classic'?83:86));
      }
      assert.ok(markup.includes(`signal-${status} signal-motion`));
      assert.equal(markup.includes('class="attention-ring"'),status==='attention');
    }
  }
});
test('static mode is explicit in all three designs without losing the active status',()=>{
  for(const style of ['classic','silver','mechanical'] as const) {
    const markup=renderToStaticMarkup(createElement(Signal,{status:'running',style,motion:false}));
    assert.ok(markup.includes('signal-running signal-static'));
    assert.ok(!markup.includes('signal-motion'));
    assert.equal((markup.match(/lamp-active/g)??[]).length,1);
  }
});
