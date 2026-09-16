package zova

func TestABIVersionAndStatusNames(t *testing.T) {
	major, minor, patch := ABIVersionNumbers()
	if major != 1 || minor != 0 || patch != 0 {
		t.Fatalf("unexpected ABI version: %d.%d.%d", major, minor, patch)
	}
	if got := ABIVersion(); got != "1.0.0-rc.1" {
		t.Fatalf("unexpected ABI version string: %q", got)
	}
	if got := StatusName(StatusOK); got != "ZOVA_OK" {
		t.Fatalf("unexpected OK status name: %q", got)
	}
	if got := StatusName(StatusExtensionUnavailable); got != "ZOVA_EXTENSION_UNAVAILABLE" {
		t.Fatalf("unexpected extension status name: %q", got)
	}
}
