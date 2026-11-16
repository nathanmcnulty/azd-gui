import { expect, it } from 'vitest';
import { classifyAzureQuotaEvidence, confirmedAzureQuotaErrorCode } from './regional-recovery';

it.each([
  ['QuotaExceeded', 'QuotaExceeded'],
  ['Deployment failed: SubscriptionIsOverQuotaForSku', 'SubscriptionIsOverQuotaForSku'],
  ['resourcequotaexceeded', 'ResourceQuotaExceeded']
] as const)('recognizes the explicit Azure quota code in %s', (evidence, expected) => {
  expect(confirmedAzureQuotaErrorCode(evidence)).toBe(expected);
});

it('recognizes the Azure Automation regional quota rejection message without retaining its text', () => {
  expect(classifyAzureQuotaEvidence('Conflict: The operation could not be completed because it exceeded your quota for Automation accounts in your subscription in the region.'))
    .toEqual({ kind: 'regional-quota-message' });
});

it.each([
  '',
  'OperationNotAllowed',
  'The deployment failed for another reason.',
  'Quota was not checked',
  'NotQuotaExceeded',
  'RegionalQuotaExceeded'
])('does not infer quota exhaustion from inconclusive evidence: %s', evidence => {
  expect(confirmedAzureQuotaErrorCode(evidence)).toBeNull();
  expect(classifyAzureQuotaEvidence(evidence)).toBeNull();
});
