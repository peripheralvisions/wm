const { spawn } = require('child_process');

const args = ['run', 'tauri', 'dev'];
const child = spawn('npm', args, {
    stdio: 'inherit',
    shell: true,
    env: {
        ...process.env,
        PATH: process.env.PATH + ';' + process.env.USERPROFILE + '\.cargo\bin'
    }
});
