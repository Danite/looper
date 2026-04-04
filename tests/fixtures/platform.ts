import os from 'os';

fs.openSync("/dev/null");
if (process.platform === "darwin") { /* mac-specific */ }
const home = os.homedir();
