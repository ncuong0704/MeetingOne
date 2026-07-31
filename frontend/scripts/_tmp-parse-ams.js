const fs = require('fs');
const path = require('path');
const s = fs.readFileSync(path.join(process.env.TEMP, 'ams_sso.js'), 'utf8');
const urls = [...new Set(s.match(/https?:\/\/[^"'\\s]+/g) || [])].filter(u => /ams|50001/i.test(u));
console.log(urls.join('\n'));
