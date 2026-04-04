import fs from 'fs';
import { exec } from 'child_process';
import os from 'os';

// env var
const dbUrl = process.env.DATABASE_URL;

// filesystem
const config = fs.readFileSync('./settings.json');

// network
fetch("https://api.internal.service/v1/data");

// shell
exec("docker compose up -d");

// ports
app.listen(4000);
new Redis("redis://cache.local:6379");

// platform
if (process.platform === "linux") {}
const home = os.homedir();
