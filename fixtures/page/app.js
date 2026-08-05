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

// Storage seeds — local/session, cookies, IndexedDB — so the storage fixture
// has something real to read after enable.
try {
  localStorage.setItem('fixture-key', 'fixture-value');
  sessionStorage.setItem('fixture-session', 'yes');
  document.cookie = 'fixture-cookie=seeded; path=/';
} catch (e) { /* file:// has no storage */ }

try {
  const open = indexedDB.open('fixture-db', 1);
  open.onupgradeneeded = (ev) => {
    const db = ev.target.result;
    if (!db.objectStoreNames.contains('items')) {
      db.createObjectStore('items', { keyPath: 'id' });
    }
  };
  open.onsuccess = (ev) => {
    const db = ev.target.result;
    const tx = db.transaction('items', 'readwrite');
    tx.objectStore('items').put({ id: 1, label: 'seeded' });
  };
} catch (e) { /* ignore */ }

// Network: a successful fetch, a deliberate 404, and a WebSocket. The WS echo
// server is started beside the static file server when recording network-load.
fetch('data.json')
  .then((r) => r.json())
  .then((d) => { console.log('fetched', d.name); })
  .catch(() => { /* fine when served from file:// */ });

fetch('missing-404.json').catch(() => { /* expected failure */ });

try {
  const ws = new WebSocket('ws://127.0.0.1:8732');
  ws.onopen = () => { ws.send('fixture-ping'); };
  ws.onmessage = (ev) => { console.log('ws', ev.data); };
  ws.onerror = () => { /* echo server may be down outside network-load */ };
} catch (e) { /* ignore */ }

setInterval(() => { state.total = computeTotal(state.items); }, 250);
