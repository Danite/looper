import fs from 'fs';
import path from 'path';

const config = fs.readFileSync('./config/app.json');
fs.writeFileSync('/tmp/cache.json', JSON.stringify(data));
const exists = fs.existsSync('./uploads');
const resolved = path.join('src', 'config', 'settings.yaml');
