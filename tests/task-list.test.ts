import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createElement} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import type {Task} from '../src/model.ts';

Object.assign(globalThis,{window:{addEventListener(){}},location:{search:''}});
const {TaskList}=await import('../src/App.tsx');

test('local question records offer confirmation without exposing it for approvals or locks',()=>{
  const task:Task={id:'codex:test',title:'测试任务',status:'attention',source:'Codex · 本机识别',message:'检测到待回答记录，请在 Codex 核对。',external:true,updatedAt:1};
  const render=(patch:Partial<Task>={})=>renderToStaticMarkup(createElement(TaskList,{tasks:[{...task,...patch}],now:1,onError(){}}));
  assert.ok(render().includes('>已处理</button>'));
  assert.ok(render().includes('移除测试任务'));
  assert.ok(!render({status:'running'}).includes('>已处理</button>'));
  assert.ok(!render({source:'Codex Hook'}).includes('>已处理</button>'));
  assert.ok(!render({message:'工具报告锁屏，请检查任务是否已恢复。'}).includes('>已处理</button>'));
});
