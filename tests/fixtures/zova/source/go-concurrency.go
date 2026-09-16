package zova

func TestConcurrentOneDBCallsAndCopiedDiagnostics(t *testing.T) {
	db, err := Create(tempZovaPath(t, "concurrent"))
	if err != nil {
		t.Fatal(err)
	}
	defer db.Close()

	must(t, db.Exec("create table events(id integer primary key, worker integer not null, seq integer not null)"))

	const workerCount = 6
	const insertsPerWorker = 12
	errs := make(chan error, workerCount)
	var wg sync.WaitGroup
	for worker := 0; worker < workerCount; worker++ {
		worker := worker
		wg.Add(1)
		go func() {
			defer wg.Done()
			for seq := 0; seq < insertsPerWorker; seq++ {
				if err := db.Exec(fmt.Sprintf(
					"insert into events(worker, seq) values (%d, %d)",
					worker,
					seq,
				)); err != nil {
					errs <- err
					return
				}
			}
		}()
	}
	wg.Wait()
	close(errs)
	for err := range errs {
		if err != nil {
			t.Fatal(err)
		}
	}
	if got := scalarInt(t, db, "select count(*) from events"); got != workerCount*insertsPerWorker {
		t.Fatalf("concurrent insert count = %d", got)
	}

	stmt, err := db.Prepare("select id as event_id, worker from events")
	if err != nil {
		t.Fatal(err)
	}
	defer stmt.Close()

	errs = make(chan error, workerCount)
	for worker := 0; worker < workerCount; worker++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for i := 0; i < 50; i++ {
				count, err := stmt.ColumnCount()
				if err != nil {
					errs <- err
					return
				}
				if count != 2 {
					errs <- fmt.Errorf("column count = %d", count)
					return
				}
				name, err := stmt.ColumnName(0)
				if err != nil {
					errs <- err
					return
				}
				if name != "event_id" {
					errs <- fmt.Errorf("column name = %q", name)
					return
				}
			}
		}()
	}
	wg.Wait()
	close(errs)
	for err := range errs {
		if err != nil {
			t.Fatal(err)
		}
	}

	err = db.Exec("select * from no_such_table")
	if err == nil {
		t.Fatal("missing table query succeeded")
	}
	copied := err.Error()
	if !strings.Contains(copied, "no_such_table") {
		t.Fatalf("diagnostic = %q", copied)
	}
	must(t, db.Exec("create table after_error(id integer)"))
	if !strings.Contains(err.Error(), "no_such_table") {
		t.Fatalf("diagnostic was not copied: %q", err.Error())
	}
}
