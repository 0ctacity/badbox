package zova

func TestObjectIDHelpersAreSHA256Identities(t *testing.T) {
	objectID := ObjectIDFor([]byte("abc"))
	chunkID := ObjectChunkIDFor([]byte("abc"))
	const want = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
	if got := hex.EncodeToString(objectID[:]); got != want {
		t.Fatalf("ObjectIDFor = %s, want %s", got, want)
	}
	if got := hex.EncodeToString(chunkID[:]); got != want {
		t.Fatalf("ObjectChunkIDFor = %s, want %s", got, want)
	}
}
