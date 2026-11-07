Set-StrictMode -Version Latest

function Get-ProjectValidationDefinition {
    [CmdletBinding()]
    param()

    New-AzdValidationCheckDefinition `
        -Id 'context.template-root' `
        -Phase context `
        -Title 'Template root is complete' `
        -Summary 'azure.yaml exists at the repository root.' `
        -SideEffect none `
        -Action {
            $repositoryRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
            if (-not (Test-Path -LiteralPath (Join-Path $repositoryRoot 'azure.yaml') -PathType Leaf)) {
                throw 'azure.yaml was not found.'
            }
        }

    New-AzdValidationCheckDefinition `
        -Id 'context.azure-cli-session' `
        -Phase context `
        -Title 'Azure CLI context is available' `
        -Summary 'An existing Azure CLI session is available for read-only validation.' `
        -SideEffect readOnly `
        -Remediation 'Run az login using the normal broker or browser flow, then rerun validation.' `
        -Action {
            if (-not $env:AZURE_SUBSCRIPTION_ID) {
                throw 'Expected Azure subscription value is missing.'
            }
            $contextJson = & az account show --subscription $env:AZURE_SUBSCRIPTION_ID --only-show-errors --output json
            if ($LASTEXITCODE -ne 0) { throw 'No usable cached Azure CLI context was found for the selected subscription.' }
            $context = $contextJson | ConvertFrom-Json
            if ([string] $context.id -ne [string] $env:AZURE_SUBSCRIPTION_ID) {
                throw 'The Azure CLI context does not match the expected subscription.'
            }
            if ($env:AZURE_TENANT_ID -and [string] $context.tenantId -ne [string] $env:AZURE_TENANT_ID) {
                throw 'The Azure CLI context does not match the expected tenant.'
            }
        }

    New-AzdValidationCheckDefinition `
        -Id 'infrastructure.resource-group' `
        -Phase infrastructure `
        -Title 'Resource group exists' `
        -Summary 'The deployed resource group is readable in the selected subscription.' `
        -SideEffect readOnly `
        -Remediation 'Confirm AZURE_RESOURCE_GROUP and the selected Azure subscription, then rerun validation.' `
        -Action {
            if (-not $env:AZURE_RESOURCE_GROUP) { throw 'AZURE_RESOURCE_GROUP is missing.' }
            if (-not $env:AZURE_SUBSCRIPTION_ID) { throw 'AZURE_SUBSCRIPTION_ID is missing.' }
            & az group show --subscription $env:AZURE_SUBSCRIPTION_ID --name $env:AZURE_RESOURCE_GROUP --only-show-errors --output none
            if ($LASTEXITCODE -ne 0) { throw 'The deployed resource group could not be read.' }
        }

}

Export-ModuleMember -Function 'Get-ProjectValidationDefinition'
