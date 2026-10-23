import { writeFileSync } from 'node:fs';
const value=process.env.SIGNALING_URL || 'http://localhost:8787';
const url=new URL(value);
if(!['http:','https:'].includes(url.protocol)) throw new Error('SIGNALING_URL must be http(s)');
writeFileSync('dist/config.json',JSON.stringify({signalingUrl:url.origin}));
