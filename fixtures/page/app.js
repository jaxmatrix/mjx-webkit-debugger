// A named function to set a breakpoint in, called on a timer so a recorded
// session reliably reaches it without needing a human to click anything.
function computeTotal(items) {
  let total = 0;
  for (const item of items) {
    total += item.value;
  }
  return total;
}

function makeItems(n) {
  const items = [];
  for (let i = 0; i < n; i += 1) {
    items.push({ id: i, value: i * 2, label: `item-${i}` });
  }
  return items;
}

const state = { items: makeItems(8), total: 0, nested: { a: { b: { c: 'deep' } } } };
state.total = computeTotal(state.items);
console.log('fixture ready, total =', state.total);

// Storage and network, so those fixtures have something to record.
try {
  localStorage.setItem('fixture-key', 'fixture-value');
  sessionStorage.setItem('fixture-session', 'yes');
} catch (e) { /* file:// has no storage */ }

fetch('data.json')
  .then((r) => r.json())
  .then((d) => { console.log('fetched', d.name); })
  .catch(() => { /* fine when served from file:// */ });

setInterval(() => { state.total = computeTotal(state.items); }, 250);
