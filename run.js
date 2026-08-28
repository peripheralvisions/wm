const { execSync } = require('child_process');
const path = require('path');
const os = require('os');

const cargoBin = path.join(os.homedir(), '.cargo', 'bin');
const env = { ...process.env, PATH: `${process.env.PATH};${cargoBin}` };

try {
  execSync('npm run tauri dev', { env, stdio: 'inherit' });
} catch (e) {
  process.exit(1);
}
