package fixture

func outer() {
	label := "🦀 go work()"
	_ = label
	// go work()
	go work()
	go func() {
		go work()
		go work()
	}()
}

func boundary() {
	for i := 0; i < 10; i++ {
		go work()
	}
}

func zero() {}

type Example struct{}

func (e Example) method() {
	go work()
	go work()
}
