const assert = require('node:assert/strict');
const test = require('node:test');
const {RequestSession} = require('./app.js');

test('RequestSession serializes operations and invalidates completed generations', () => {
  const requests = new RequestSession();
  const initial = requests.begin();

  assert.equal(initial, 1);
  assert.equal(requests.busy, true);
  assert.equal(requests.begin(), null, 'a reset cannot overlap an in-flight move or load');
  assert.equal(requests.isCurrent(initial), true);

  assert.equal(requests.finish(initial), true);
  const reset = requests.begin();

  assert.equal(reset, 2);
  assert.equal(requests.isCurrent(initial), false, 'a late initial response cannot replace reset state');
  assert.equal(requests.isCurrent(reset), true);
  assert.equal(requests.finish(initial), false, 'a late completion cannot clear the active request');
  assert.equal(requests.finish(reset), true);
  assert.equal(requests.busy, false);
});
