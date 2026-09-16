package zova

func TestBackupCompactAndRestore(t *testing.T) {
	source := tempZovaPath(t, "ops-source")
	backup := tempZovaPath(t, "ops-backup")
	compact := tempZovaPath(t, "ops-compact")
	restored := tempZovaPath(t, "ops-restored")
	noVerify := tempZovaPath(t, "ops-no-verify")
	db, err := Create(source)
	if err != nil {
		t.Fatal(err)
	}
	defer db.Close()

	must(t, db.Exec("create table records(id integer primary key, body text not null)"))
	must(t, db.Exec("insert into records(body) values ('kept')"))
	objectID, err := db.PutObject([]byte("go operational object bytes"))
	if err != nil {
		t.Fatal(err)
	}
	must(t, db.CreateVectorCollection("chunks", VectorCollectionOptions{
		Dimensions:  2,
		Metric:      VectorMetricL2,
		ElementType: VectorElementTypeF32,
	}))
	must(t, db.PutVector("chunks", "near", VectorValues{ElementType: VectorElementTypeF32, F32: []float32{0, 0}}))
	must(t, db.PutVector("chunks", "far", VectorValues{ElementType: VectorElementTypeF32, F32: []float32{10, 0}}))

	must(t, db.BackupTo(backup))
	must(t, db.CompactTo(compact))
	must(t, db.BackupTo(noVerify, BackupOptions{NoVerify: true}))
	if err := db.BackupTo(backup); !errorStatusIs(err, StatusDestinationExists) {
		t.Fatalf("backup destination conflict = %v, want StatusDestinationExists", err)
	}
	if err := db.CompactTo("bad-destination.db"); !errorStatusIs(err, StatusNotZovaPath) {
		t.Fatalf("compact bad destination = %v, want StatusNotZovaPath", err)
	}
	if err := db.BackupTo(tempZovaPath(t, "too-many-options"), BackupOptions{}, BackupOptions{}); !errorStatusIs(err, StatusInvalidArgument) {
		t.Fatalf("too many backup options = %v, want StatusInvalidArgument", err)
	}

	must(t, RestoreBackup(backup, restored))
	if err := RestoreBackup("bad-source.db", tempZovaPath(t, "bad-restore")); !errorStatusIs(err, StatusNotZovaPath) {
		t.Fatalf("restore bad source = %v, want StatusNotZovaPath", err)
	}

	for _, path := range []string{backup, compact, restored, noVerify} {
		verifyOperationalCopy(t, path, objectID)
	}
}
