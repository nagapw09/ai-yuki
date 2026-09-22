import fs from 'node:fs'
const version = JSON.parse(fs.readFileSync('package.json', 'utf8')).version
const lock = JSON.parse(fs.readFileSync('package-lock.json', 'utf8'))
lock.version = version
for (const name of ['', 'apps/desktop', 'core']) lock.packages[name].version = version
fs.writeFileSync('package-lock.json', JSON.stringify(lock, null, 2) + '\n')
