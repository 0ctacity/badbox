package zova

func tempZovaPath(t *testing.T, name string) string {
	t.Helper()
	return filepath.Join(t.TempDir(), name+".zova")
}
