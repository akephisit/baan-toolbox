import assert from 'node:assert/strict';

const frontend = process.env.ALLOWED_ORIGINS?.split(',')[0];
const signaling = process.env.SIGNALING_URL;
assert(frontend && signaling, 'Set ALLOWED_ORIGINS and SIGNALING_URL');

// Retry while the new deployment propagates to Cloudflare's edge.
let failure;
for (let attempt = 0; attempt < 6; attempt++) {
  try {
    const configResponse = await fetch(`${frontend}/config.json`, { cache: 'no-store', signal: AbortSignal.timeout(15000) });
    assert(configResponse.ok, `Frontend returned ${configResponse.status}`);
    const config = await configResponse.json();
    assert.equal(config.signalingUrl, signaling);
    const health = await fetch(`${signaling}/health`, { signal: AbortSignal.timeout(15000) });
    assert(health.ok, `Signaling returned ${health.status}`);
    console.log(`Deployment reachable: ${frontend} and ${signaling}`);
    process.exit(0);
  } catch (error) {
    failure = error;
    if (attempt < 5) await new Promise(resolve => setTimeout(resolve, 5000));
  }
}
throw failure;
