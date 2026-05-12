package storage

import (
	"database/sql"
	"fmt"
	"time"

	_ "modernc.org/sqlite"
)

// Quarter is a single 15-minute price point.
type Quarter struct {
	Ts    time.Time // UTC
	Price float32   // EUR/MWh
}

// DB wraps a SQLite database holding OTE day-ahead prices.
type DB struct {
	sql *sql.DB
	loc *time.Location
}

const schema = `
CREATE TABLE IF NOT EXISTS prices (
  ts          INTEGER NOT NULL PRIMARY KEY,
  prague_date TEXT    NOT NULL,
  price       REAL    NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_prices_prague_date ON prices(prague_date);
`

// Open opens (or creates) the SQLite database at path and ensures the schema.
func Open(path string) (*DB, error) {
	sqlDB, err := sql.Open("sqlite", path+"?_pragma=journal_mode(WAL)&_pragma=synchronous(NORMAL)&_pragma=busy_timeout(5000)")
	if err != nil {
		return nil, fmt.Errorf("open sqlite: %w", err)
	}
	if _, err := sqlDB.Exec(schema); err != nil {
		sqlDB.Close()
		return nil, fmt.Errorf("init schema: %w", err)
	}
	loc, err := time.LoadLocation("Europe/Prague")
	if err != nil {
		loc = time.UTC
	}
	return &DB{sql: sqlDB, loc: loc}, nil
}

// Close releases the underlying database connection.
func (db *DB) Close() error {
	return db.sql.Close()
}

// PragueDate formats a UTC instant as a Prague-local YYYY-MM-DD string.
func (db *DB) PragueDate(ts time.Time) string {
	return ts.In(db.loc).Format("2006-01-02")
}

// HasDay reports whether any rows exist for the given Prague-local date.
func (db *DB) HasDay(pragueDate string) (bool, error) {
	var n int
	err := db.sql.QueryRow(`SELECT COUNT(*) FROM prices WHERE prague_date = ?`, pragueDate).Scan(&n)
	if err != nil {
		return false, err
	}
	return n > 0, nil
}

// GetDay returns all quarter-hour prices for the given Prague-local date, ordered by time.
func (db *DB) GetDay(pragueDate string) ([]Quarter, error) {
	rows, err := db.sql.Query(`SELECT ts, price FROM prices WHERE prague_date = ? ORDER BY ts`, pragueDate)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var out []Quarter
	for rows.Next() {
		var unix int64
		var price float64
		if err := rows.Scan(&unix, &price); err != nil {
			return nil, err
		}
		out = append(out, Quarter{
			Ts:    time.Unix(unix, 0).UTC(),
			Price: float32(price),
		})
	}
	return out, rows.Err()
}

// SaveQuarters writes all quarters in a single transaction. Existing rows for
// the same timestamp are replaced.
func (db *DB) SaveQuarters(quarters []Quarter) error {
	if len(quarters) == 0 {
		return nil
	}
	tx, err := db.sql.Begin()
	if err != nil {
		return err
	}
	stmt, err := tx.Prepare(`INSERT OR REPLACE INTO prices(ts, prague_date, price) VALUES (?, ?, ?)`)
	if err != nil {
		tx.Rollback()
		return err
	}
	defer stmt.Close()

	for _, q := range quarters {
		if _, err := stmt.Exec(q.Ts.Unix(), db.PragueDate(q.Ts), q.Price); err != nil {
			tx.Rollback()
			return err
		}
	}
	return tx.Commit()
}

// MonthAverages returns the raw EUR average price for each Prague-local date in
// the inclusive range. Days with no rows are absent from the map.
func (db *DB) MonthAverages(pragueDateFrom, pragueDateTo string) (map[string]float32, error) {
	rows, err := db.sql.Query(
		`SELECT prague_date, AVG(price) FROM prices WHERE prague_date BETWEEN ? AND ? GROUP BY prague_date`,
		pragueDateFrom, pragueDateTo,
	)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	out := make(map[string]float32)
	for rows.Next() {
		var date string
		var avg float64
		if err := rows.Scan(&date, &avg); err != nil {
			return nil, err
		}
		out[date] = float32(avg)
	}
	return out, rows.Err()
}
