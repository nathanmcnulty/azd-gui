# App-owned correlation using the existing evidence v1 contract.
$report.schemaVersion = '1.1'
$report.environment.metadata = @{
    azdManagementEvidence = @{
        version = '1'; evidenceClass = 'validation'
        binding = @{
            project = @{ id = $env:AZD_GUI_PROJECT_ID; environment = $env:AZD_GUI_ENVIRONMENT }
            target = @{ azureCloud = $env:AZD_GUI_AZURE_CLOUD; tenantId = $env:AZURE_TENANT_ID; subscriptionId = $env:AZURE_SUBSCRIPTION_ID; resourceGroup = $env:AZURE_RESOURCE_GROUP }
            source = @{ templateId = $env:AZD_GUI_TEMPLATE_ID; revision = $env:AZD_GUI_SOURCE_REVISION; contractDigest = $env:AZD_GUI_CONTRACT_DIGEST }
            operation = @{ id = $env:AZD_GUI_OPERATION_ID; kind = 'validate' }
        }
        nextActions = @(@{ code = 'reviewWarnings'; owner = 'operator'; priority = 'recommended' })
    }
}
