// Dedicated worker used by the target-multiplexed fixture. It must stay alive
// long enough for Target.targetCreated / Worker.workerCreated to fire and for
// the recorder to send a wrapped Runtime.enable.
self.postMessage({ ready: true, from: 'fixture-worker' });
setInterval(() => {
  self.postMessage({ tick: Date.now() });
}, 500);
