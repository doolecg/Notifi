// Signs update installers so Notifi only installs files made by whoever holds the key.
//   node scripts/sign-update.mjs --new-key   make a key pair: the private key goes into .env, the public key is printed
//   node scripts/sign-update.mjs <file>...   write <file>.sig (Ed25519 signature, hex) next to each file
// Upload each .sig to the GitHub release beside its installer. Keep a backup of .env: without the key,
// installed copies cannot update to new builds and have to be updated by hand.
import { generateKeyPairSync, createPrivateKey, sign } from 'node:crypto';
import { existsSync, readFileSync, writeFileSync, appendFileSync } from 'node:fs';
import { resolve } from 'node:path';

const ENV = resolve(import.meta.dirname, '..', '.env');
const NAME = 'NOTIFI_UPDATE_KEY';

function savedKey() {
  const line = existsSync(ENV) && readFileSync(ENV, 'utf8').split(/\r?\n/).find((l) => l.startsWith(`${NAME}=`));
  return line ? line.slice(NAME.length + 1).trim() : null;
}

const args = process.argv.slice(2);
if (args[0] === '--new-key') {
  if (savedKey()) throw new Error(`${NAME} is already in .env; remove it by hand first if you really want a new key`);
  const { publicKey, privateKey } = generateKeyPairSync('ed25519');
  const der = privateKey.export({ type: 'pkcs8', format: 'der' }).toString('base64');
  appendFileSync(ENV, `${existsSync(ENV) ? '\n' : ''}${NAME}=${der}\n`);
  // The raw 32-byte key is the end of the SPKI encoding.
  console.log(publicKey.export({ type: 'spki', format: 'der' }).subarray(-32).toString('hex'));
} else if (args.length) {
  const der = savedKey();
  if (!der) throw new Error(`No ${NAME} in .env; run with --new-key first`);
  const key = createPrivateKey({ key: Buffer.from(der, 'base64'), format: 'der', type: 'pkcs8' });
  for (const file of args) {
    writeFileSync(`${file}.sig`, sign(null, readFileSync(file), key).toString('hex'));
    console.log(`signed ${file}`);
  }
} else {
  console.log('usage: node scripts/sign-update.mjs --new-key | <file>...');
}
