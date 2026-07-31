/**
 * Apply staged defaults from scripts/.export-staging into builtin app files.
 * Staging is populated from browser localStorage (dev UI) or manual edits.
 */
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const stagingDir = path.join(root, 'scripts', '.export-staging');
const promptsRsPath = path.join(root, 'src-tauri', 'src', 'summary', 'prompts.rs');

function copyTemplate(id) {
  const src = path.join(stagingDir, `${id}.json`);
  if (!fs.existsSync(src)) {
    console.warn('Skip missing staging template:', id);
    return;
  }
  const content = fs.readFileSync(src, 'utf8');
  fs.writeFileSync(path.join(root, 'public', 'dev-templates', `${id}.json`), content, 'utf8');
  fs.writeFileSync(path.join(root, 'src-tauri', 'templates', `${id}.json`), content, 'utf8');
  console.log('Updated template:', id);
}

function toRustRawString(value) {
  const trimmed = value.trimEnd();
  let hashCount = 1;
  const close = () => `"#${'#'.repeat(hashCount)}`;
  while (trimmed.includes(close())) {
    hashCount += 1;
  }
  const open = `r${'#'.repeat(hashCount)}"`;
  const end = `${'#'.repeat(hashCount)}"`;
  return `${open}${trimmed}${end}`;
}

function updatePromptsRs(promptText) {
  const source = fs.readFileSync(promptsRsPath, 'utf8');
  const rustLiteral = toRustRawString(promptText);
  const replacement = `pub const SYSTEM_PROMPT_FINAL_TEMPLATE: &str = ${rustLiteral};`;
  const updated = source.replace(
    /pub const SYSTEM_PROMPT_FINAL_TEMPLATE: &str = r#+[\s\S]*?"#+;/,
    replacement
  );
  if (updated === source) {
    throw new Error('Failed to update SYSTEM_PROMPT_FINAL_TEMPLATE in prompts.rs');
  }
  fs.writeFileSync(promptsRsPath, updated, 'utf8');
  console.log('Updated prompts.rs');
}

const templateIds = fs
  .readdirSync(stagingDir)
  .filter(name => name.endsWith('.json') && name !== 'prompt.json')
  .map(name => name.replace(/\.json$/, ''));

for (const id of templateIds) {
  copyTemplate(id);
}

const promptStaging = path.join(stagingDir, 'prompt.json');
if (fs.existsSync(promptStaging)) {
  const promptConfig = JSON.parse(fs.readFileSync(promptStaging, 'utf8'));
  const promptText = promptConfig.systemPromptFinalTemplate.trimEnd();
  fs.writeFileSync(
    path.join(root, 'public', 'dev-prompt-defaults.json'),
    JSON.stringify({ systemPromptFinalTemplate: promptText }, null, 2) + '\n',
    'utf8'
  );
  console.log('Updated dev-prompt-defaults.json');
  updatePromptsRs(promptText);
}

console.log('Done.');
