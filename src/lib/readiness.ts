import type { TemplateManifest, ToolStatus } from './types';

/** Presentation only: the existing requirement snapshot remains the execution gate. */
export function toolReadiness(tool: ToolStatus, manifest?: TemplateManifest) {
  // Native statuses describe detection; conditions live in the reviewed manifest.
  const condition = tool.condition ?? manifest?.prerequisites.find(item => item.id === tool.id)?.condition;
  const field = manifest?.configuration.groups.flatMap(group => group.fields)
    .find(field => field.id === condition?.field);
  const choice = condition && field && !field.sensitive && field.type !== 'secret'
    ? ` for ${field.label} = ${String(condition.equals)}` : '';
  const requiredVersion = [...new Set(tool.minimumVersion?.split(',').map(value => value.trim()))].join(', ') || 'any version';
  const purpose = tool.id === 'azd' ? 'Prepares the project and runs the reviewed deployment command'
    : tool.id === 'pwsh' ? 'Runs the template’s PowerShell hooks on this computer'
    : tool.kind === 'powershellModule' ? 'Used by the template’s PowerShell setup hooks'
    : 'Used by the template’s local deployment workflow';
  const detected = tool.state === 'checking' ? 'Checking this selection…'
    : tool.state === 'unknown' ? 'Detection could not be verified'
    : tool.state === 'missing' ? 'Not installed or not found'
    : `Detected ${tool.installedVersion ?? 'installation'}; required ${requiredVersion}`;
  const action = tool.state === 'ready' ? 'Continue; no tool change needed'
    : tool.state === 'checking' ? 'Wait for the current check'
    : tool.state === 'unknown' ? 'Check again; view instructions if detection still fails'
    : tool.state === 'outdated' ? 'Update to the required version, then check again'
    : tool.canInstall ? 'Review installation before installing'
    : 'Use the installation instructions, then check again';
  return { why: `${purpose}${choice}. ${tool.required ? 'Required for these choices.' : 'Optional; does not block continuing.'}`,
    detected, requiredVersion, action: `${action}${!tool.required && tool.state !== 'ready' ? ' (optional)' : ''}` };
}
