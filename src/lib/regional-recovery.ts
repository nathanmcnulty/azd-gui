/** Azure error codes that explicitly report a quota limit. Other deployment errors remain inconclusive. */
const confirmedQuotaErrorCodes = [
  'QuotaExceeded',
  'SubscriptionIsOverQuotaForSku',
  'ResourceQuotaExceeded'
] as const;

export type ConfirmedQuotaErrorCode = typeof confirmedQuotaErrorCodes[number];
export type AzureQuotaEvidence =
  | { kind: 'error-code'; code: ConfirmedQuotaErrorCode }
  | { kind: 'regional-quota-message' };

/**
 * Recognize only an explicit Azure quota error code from operator-provided terminal evidence.
 * This is not a live quota check and callers must retain that distinction in their UI.
 */
export function confirmedAzureQuotaErrorCode(value: string): ConfirmedQuotaErrorCode | null {
  for (const code of confirmedQuotaErrorCodes) {
    if (new RegExp(`\\b${code}\\b`, 'i').test(value)) return code;
  }
  return null;
}

/** Accept a short Azure message only when it explicitly names a regional Automation Account quota. */
export function classifyAzureQuotaEvidence(value: string): AzureQuotaEvidence | null {
  const code = confirmedAzureQuotaErrorCode(value);
  if (code) return { kind: 'error-code', code };
  const explicitRegionalAutomationQuota = /\bquota\b/i.test(value)
    && /\b(exceed(?:ed|s|ing)?|over|limit)\b/i.test(value)
    && /\b(region|regional|location)\b/i.test(value)
    && /\bautomation\s+accounts?\b/i.test(value);
  return explicitRegionalAutomationQuota ? { kind: 'regional-quota-message' } : null;
}
