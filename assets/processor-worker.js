// Bootstrap only. File I/O, hashing, validation and storage live in Rust/WASM.
import init, { dispatch } from './processor/toolbox_processor.js';
const ready = init();
let queue = Promise.resolve();
self.onmessage = ({ data }) => {
  queue = queue.then(async () => {
    try {
      await ready;
      const result = await dispatch(data);
      const transfer = result?.bytes instanceof ArrayBuffer ? [result.bytes] : [];
      self.postMessage({ requestId: data.requestId, ok: true, data: result }, transfer);
    } catch (error) {
      self.postMessage({ requestId: data.requestId, ok: false, code: error?.name || '', error: String(error?.message || error) });
    }
  });
};
