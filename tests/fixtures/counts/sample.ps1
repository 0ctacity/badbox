function Invoke-Repeatedly {
    $first = "Get-Date"
    $second = "Get-Location"
    Invoke-Expression $first
    Invoke-Expression $second
}

function Invoke-Once {
    $command = "Get-Date"
    Invoke-Expression $command
}
