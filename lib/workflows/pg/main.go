package main

import (
	"compress/gzip"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"
	"github.com/shopspring/decimal"
	"github.com/spf13/cobra"
)

// Config holds the PostgreSQL connection and container settings.
type Config struct {
	Container string `json:"container"`
	Network   string `json:"network"`
	Image     string `json:"image"`
	Password  string `json:"password"`
	User      string `json:"user"`
	Port      int    `json:"port"`
	Host      string `json:"host"`
	Database  string `json:"database"`
	DataDir   string `json:"data_dir"`
}

var cfg Config

func findPgConfig() Config {
	c := Config{
		Container: "pg1",
		Network:   "pgnet",
		Image:     "docker.io/library/postgres:17",
		Password:  "t6drtfyig7",
		User:      "admin",
		Port:      5432,
		Host:      "localhost",
		Database:  "postgres",
		DataDir:   filepath.Join(os.Getenv("HOME"), ".local/share/pg/data"),
	}

	dir, err := os.Getwd()
	if err != nil {
		return c
	}
	for {
		candidate := filepath.Join(dir, ".pg.json")
		if data, err := os.ReadFile(candidate); err == nil {
			_ = json.Unmarshal(data, &c)
			if c.Container == "" {
				c.Container = "pg1"
			}
			if c.Network == "" {
				c.Network = "pgnet"
			}
			if c.Image == "" {
				c.Image = "docker.io/library/postgres:17"
			}
			if c.Password == "" {
				c.Password = "t6drtfyig7"
			}
			if c.User == "" {
				c.User = "admin"
			}
			if c.Port == 0 {
				c.Port = 5432
			}
			if c.Host == "" {
				c.Host = "localhost"
			}
			if c.Database == "" {
				c.Database = "postgres"
			}
			if c.DataDir == "" {
				c.DataDir = filepath.Join(os.Getenv("HOME"), ".local/share/pg/data")
			}
			return c
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			break
		}
		dir = parent
	}
	return c
}

func envOr(key, fallback string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return fallback
}

func connString(dbname string) string {
	if dbname == "" {
		dbname = envOr("PGDATABASE", cfg.Database)
	}
	return fmt.Sprintf("host=%s port=%s user=%s password=%s dbname=%s",
		envOr("PGHOST", cfg.Host),
		envOr("PGPORT", fmt.Sprintf("%d", cfg.Port)),
		envOr("PGUSER", cfg.User),
		envOr("PGPASSWORD", cfg.Password),
		dbname,
	)
}

func getConn(ctx context.Context, dbname string) *pgx.Conn {
	conn, err := pgx.Connect(ctx, connString(dbname))
	if err != nil {
		fmt.Fprintf(os.Stderr, "Connection failed: %v\n", err)
		os.Exit(1)
	}
	return conn
}

// ---------- podman helpers ----------

func networkExists(name string) bool {
	return exec.Command("podman", "network", "exists", name).Run() == nil
}

func createNetwork(name string) bool {
	return exec.Command("podman", "network", "create", name).Run() == nil
}

func ensureNetwork(name string) bool {
	if networkExists(name) {
		return true
	}
	fmt.Fprintf(os.Stderr, "Network '%s' does not exist. Creating...\n", name)
	if createNetwork(name) {
		fmt.Fprintf(os.Stderr, "Network '%s' created successfully.\n", name)
		return true
	}
	fmt.Fprintf(os.Stderr, "Error: Failed to create network '%s'.\n", name)
	return false
}

func containerState(name string) string {
	out, err := exec.Command("podman", "ps", "-a", "--filter", fmt.Sprintf("name=^%s$", name), "--format", "{{.State}}").Output()
	if err != nil {
		return ""
	}
	return strings.TrimSpace(string(out))
}

func startContainer(name string) bool {
	return exec.Command("podman", "start", name).Run() == nil
}

func stopContainer(name string) bool {
	return exec.Command("podman", "stop", name).Run() == nil
}

func removeContainer(name string) bool {
	return exec.Command("podman", "rm", "-f", name).Run() == nil
}

// ---------- identifier quoting ----------

func quoteIdent(s string) string {
	return `"` + strings.ReplaceAll(s, `"`, `""`) + `"`
}

func qualifiedTable(schema, table string) string {
	return quoteIdent(schema) + "." + quoteIdent(table)
}

// ---------- JSON output helpers ----------

func jsonSerialize(rows []map[string]any) string {
	for _, row := range rows {
		for k, v := range row {
			switch val := v.(type) {
			case time.Time:
				row[k] = val.Format(time.RFC3339)
			case decimal.Decimal:
				f, _ := val.Float64()
				row[k] = f
			case []byte:
				row[k] = fmt.Sprintf("%x", val)
			}
		}
	}
	data, _ := json.MarshalIndent(rows, "", "  ")
	return string(data)
}

func printResults(rows []map[string]any) {
	if len(rows) == 0 {
		fmt.Fprintln(os.Stderr, "No rows returned.")
		return
	}
	// Collect ordered headers from first row
	headers := make([]string, 0)
	for k := range rows[0] {
		headers = append(headers, k)
	}
	sort.Strings(headers)

	widths := make(map[string]int)
	for _, h := range headers {
		widths[h] = len(h)
	}
	for _, row := range rows {
		for _, h := range headers {
			s := "NULL"
			if row[h] != nil {
				s = fmt.Sprintf("%v", row[h])
			}
			if len(s) > widths[h] {
				widths[h] = len(s)
			}
		}
	}

	// Header
	parts := make([]string, len(headers))
	seps := make([]string, len(headers))
	for i, h := range headers {
		parts[i] = fmt.Sprintf("%-*s", widths[h], h)
		seps[i] = strings.Repeat("-", widths[h])
	}
	fmt.Println(strings.Join(parts, " | "))
	fmt.Println(strings.Join(seps, "-|-"))

	for _, row := range rows {
		for i, h := range headers {
			s := "NULL"
			if row[h] != nil {
				s = fmt.Sprintf("%v", row[h])
			}
			parts[i] = fmt.Sprintf("%-*s", widths[h], s)
		}
		fmt.Println(strings.Join(parts, " | "))
	}
}

// collectRows converts pgx.Rows into []map[string]any.
func collectRows(r pgx.Rows) ([]map[string]any, error) {
	defer r.Close()
	descs := r.FieldDescriptions()
	var result []map[string]any
	for r.Next() {
		vals, err := r.Values()
		if err != nil {
			return nil, err
		}
		row := make(map[string]any, len(descs))
		for i, d := range descs {
			row[string(d.Name)] = vals[i]
		}
		result = append(result, row)
	}
	return result, r.Err()
}

// ---------- column type resolver ----------

func resolveColumnType(typeName string, isFloat, isDouble, isLong, isUnsigned bool, fixed int) string {
	if typeName == "number" {
		if fixed >= 0 {
			return fmt.Sprintf("NUMERIC(38,%d)", fixed)
		}
		if isDouble {
			return "DOUBLE PRECISION"
		}
		if isFloat {
			return "REAL"
		}
		if isLong {
			return "BIGINT"
		}
		return "INTEGER"
	}
	m := map[string]string{
		"string": "TEXT",
		"bool":   "BOOLEAN",
		"jsonb":  "JSONB",
	}
	if t, ok := m[typeName]; ok {
		return t
	}
	fmt.Fprintf(os.Stderr, "Error: Unknown type '%s'. Use: string, number, bool, jsonb\n", typeName)
	os.Exit(1)
	return ""
}

// ---------- CLI ----------

func main() {
	cfg = findPgConfig()

	root := &cobra.Command{
		Use:   "pg",
		Short: "PostgreSQL CLI tool with interactive mode and table commands",
		RunE: func(cmd *cobra.Command, args []string) error {
			fi, _ := os.Stdin.Stat()
			if (fi.Mode() & os.ModeCharDevice) == 0 {
				// piped stdin
				return runQueryFromReader(os.Stdin, "", "json")
			}
			return cmd.Help()
		},
		SilenceUsage: true,
	}

	root.AddCommand(kickstartCmd())
	root.AddCommand(tabCmd())
	root.AddCommand(colCmd())
	root.AddCommand(idxCmd())
	root.AddCommand(rowCmd())
	root.AddCommand(schemaCmd())
	root.AddCommand(dumpCmd())
	root.AddCommand(queryCmd())
	root.AddCommand(dbCmd())

	if err := root.Execute(); err != nil {
		os.Exit(1)
	}
}

// ==================== kickstart ====================

func kickstartCmd() *cobra.Command {
	var name, image, dataDir, network string
	var port int
	var pull, force bool

	cmd := &cobra.Command{
		Use:   "kickstart",
		Short: "Start a PostgreSQL instance using podman",
		RunE: func(cmd *cobra.Command, args []string) error {
			state := containerState(name)

			if state != "" && !force {
				if state == "running" {
					fmt.Fprintf(os.Stderr, "Container '%s' is already running.\n", name)
					return nil
				}
				fmt.Fprintf(os.Stderr, "Container '%s' exists (state: %s). Starting...\n", name, state)
				if startContainer(name) {
					fmt.Fprintf(os.Stderr, "Container '%s' started.\n", name)
					return nil
				}
				fmt.Fprintf(os.Stderr, "Error: Failed to start container '%s'.\n", name)
				os.Exit(1)
			}

			if state != "" && force {
				fmt.Fprintf(os.Stderr, "Removing existing container '%s'...\n", name)
				removeContainer(name)
			}

			if !ensureNetwork(network) {
				os.Exit(1)
			}

			if pull {
				fmt.Fprintf(os.Stderr, "Pulling image %s...\n", image)
				exec.Command("podman", "pull", image).Run()
			}

			os.MkdirAll(dataDir, 0755)

			podmanArgs := []string{
				"run", "-d",
				"--name", name,
				"--hostname", name,
				"--network", network,
				"--restart", "unless-stopped",
				"-p", fmt.Sprintf("%d:5432", port),
				"-v", fmt.Sprintf("%s:/var/lib/postgresql/data", dataDir),
				"-e", fmt.Sprintf("POSTGRES_PASSWORD=%s", cfg.Password),
				"-e", fmt.Sprintf("POSTGRES_USER=%s", cfg.User),
				image,
			}

			fmt.Fprintf(os.Stderr, "Starting PostgreSQL container '%s' on port %d...\n", name, port)
			out, err := exec.Command("podman", podmanArgs...).CombinedOutput()
			if err != nil {
				fmt.Fprintf(os.Stderr, "Error starting container: %s\n", strings.TrimSpace(string(out)))
				os.Exit(1)
			}

			fmt.Fprintf(os.Stderr, "PostgreSQL container '%s' started successfully.\n", name)
			fmt.Fprintf(os.Stderr, "\nQuick commands:\n")
			fmt.Fprintf(os.Stderr, "  Logs   : podman logs -f %s\n", name)
			fmt.Fprintf(os.Stderr, "  Stop   : podman stop %s\n", name)
			fmt.Fprintf(os.Stderr, "  Remove : podman rm -f %s\n", name)
			fmt.Fprintf(os.Stderr, "  Connect: psql -h localhost -p %d -U %s\n", port, cfg.User)
			return nil
		},
	}
	cmd.Flags().StringVarP(&name, "name", "n", cfg.Container, "Container name")
	cmd.Flags().IntVarP(&port, "port", "p", cfg.Port, "Host port")
	cmd.Flags().StringVar(&image, "image", cfg.Image, "PostgreSQL image")
	cmd.Flags().StringVar(&dataDir, "data-dir", cfg.DataDir, "Data directory")
	cmd.Flags().StringVar(&network, "network", cfg.Network, "Podman network")
	cmd.Flags().BoolVar(&pull, "pull", true, "Pull image before starting")
	cmd.Flags().BoolVar(&force, "force", false, "Remove existing container and start fresh")
	return cmd
}

// ==================== tab (table) ====================

func tabCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "tab",
		Short: "Commands related to tables",
	}
	cmd.AddCommand(tabListCmd(), tabAddCmd(), tabDropCmd(), tabRenameCmd(), tabCopyCmd())
	return cmd
}

func tabListCmd() *cobra.Command {
	var database, schema string
	cmd := &cobra.Command{
		Use:   "list",
		Short: "List all tables in a schema",
		RunE: func(cmd *cobra.Command, args []string) error {
			ctx := context.Background()
			conn := getConn(ctx, database)
			defer conn.Close(ctx)

			rows, err := conn.Query(ctx,
				"SELECT tablename FROM pg_tables WHERE schemaname = $1 ORDER BY tablename", schema)
			if err != nil {
				return err
			}
			results, err := collectRows(rows)
			if err != nil {
				return err
			}
			if len(results) > 0 {
				fmt.Fprintf(os.Stderr, "Tables in %s.%s:\n", database, schema)
				for _, r := range results {
					fmt.Fprintf(os.Stderr, "  • %s\n", r["tablename"])
				}
			} else {
				fmt.Fprintf(os.Stderr, "No tables found in %s.%s.\n", database, schema)
			}
			return nil
		},
	}
	cmd.Flags().StringVarP(&database, "database", "d", cfg.Database, "Database name")
	cmd.Flags().StringVarP(&schema, "schema", "s", "public", "Schema name")
	return cmd
}

func tabAddCmd() *cobra.Command {
	var database, schema string
	cmd := &cobra.Command{
		Use:   "add NAME",
		Short: "Create a new table with an auto-increment bigint primary key",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			name := args[0]
			ctx := context.Background()
			conn := getConn(ctx, database)
			defer conn.Close(ctx)

			q := fmt.Sprintf("CREATE TABLE IF NOT EXISTS %s (id BIGSERIAL PRIMARY KEY)", qualifiedTable(schema, name))
			if _, err := conn.Exec(ctx, q); err != nil {
				return err
			}
			fmt.Fprintf(os.Stderr, "Table '%s.%s' created.\n", schema, name)
			return nil
		},
	}
	cmd.Flags().StringVarP(&database, "database", "d", cfg.Database, "Database name")
	cmd.Flags().StringVarP(&schema, "schema", "s", "public", "Schema name")
	return cmd
}

func tabDropCmd() *cobra.Command {
	var database, schema string
	cmd := &cobra.Command{
		Use:   "drop NAME",
		Short: "Drop a table",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			name := args[0]
			ctx := context.Background()
			conn := getConn(ctx, database)
			defer conn.Close(ctx)

			q := fmt.Sprintf("DROP TABLE IF EXISTS %s", qualifiedTable(schema, name))
			if _, err := conn.Exec(ctx, q); err != nil {
				return err
			}
			fmt.Fprintf(os.Stderr, "Table '%s.%s' dropped.\n", schema, name)
			return nil
		},
	}
	cmd.Flags().StringVarP(&database, "database", "d", cfg.Database, "Database name")
	cmd.Flags().StringVarP(&schema, "schema", "s", "public", "Schema name")
	return cmd
}

func tabRenameCmd() *cobra.Command {
	var database, schema string
	cmd := &cobra.Command{
		Use:   "rename OLD_NAME NEW_NAME",
		Short: "Rename a table",
		Args:  cobra.ExactArgs(2),
		RunE: func(cmd *cobra.Command, args []string) error {
			oldName, newName := args[0], args[1]
			ctx := context.Background()
			conn := getConn(ctx, database)
			defer conn.Close(ctx)

			tx, err := conn.Begin(ctx)
			if err != nil {
				return err
			}
			defer tx.Rollback(ctx)

			// Check if already in desired state
			var newExists, oldExists bool
			tx.QueryRow(ctx,
				"SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_schema=$1 AND table_name=$2)", schema, newName).Scan(&newExists)
			tx.QueryRow(ctx,
				"SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_schema=$1 AND table_name=$2)", schema, oldName).Scan(&oldExists)

			if newExists && !oldExists {
				fmt.Fprintf(os.Stderr, "Table '%s.%s' already exists. Nothing to do.\n", schema, newName)
				return nil
			}

			_, err = tx.Exec(ctx, fmt.Sprintf("ALTER TABLE %s RENAME TO %s", qualifiedTable(schema, oldName), quoteIdent(newName)))
			if err != nil {
				return err
			}

			// Rename associated sequences
			seqRows, err := tx.Query(ctx, `
				SELECT s.relname AS seq_name
				FROM pg_class s
				JOIN pg_namespace n ON n.oid = s.relnamespace
				JOIN pg_depend d ON d.objid = s.oid
				JOIN pg_class t ON t.oid = d.refobjid
				WHERE s.relkind = 'S'
				  AND n.nspname = $1
				  AND t.relname = $2
				  AND d.deptype = 'a'`, schema, newName)
			if err != nil {
				return err
			}
			seqResults, _ := collectRows(seqRows)
			for _, sr := range seqResults {
				oldSeq := fmt.Sprintf("%v", sr["seq_name"])
				prefix := oldName + "_"
				if strings.HasPrefix(oldSeq, prefix) {
					newSeq := newName + "_" + oldSeq[len(prefix):]
					_, err = tx.Exec(ctx, fmt.Sprintf("ALTER SEQUENCE %s.%s RENAME TO %s",
						quoteIdent(schema), quoteIdent(oldSeq), quoteIdent(newSeq)))
					if err != nil {
						return err
					}
					fmt.Fprintf(os.Stderr, "Sequence '%s' renamed to '%s'.\n", oldSeq, newSeq)
				}
			}

			if err := tx.Commit(ctx); err != nil {
				return err
			}
			fmt.Fprintf(os.Stderr, "Table '%s.%s' renamed to '%s.%s'.\n", schema, oldName, schema, newName)
			return nil
		},
	}
	cmd.Flags().StringVarP(&database, "database", "d", cfg.Database, "Database name")
	cmd.Flags().StringVarP(&schema, "schema", "s", "public", "Schema name")
	return cmd
}

func tabCopyCmd() *cobra.Command {
	var database, schema string
	var withData bool
	cmd := &cobra.Command{
		Use:   "copy SOURCE DESTINATION",
		Short: "Copy a table structure (and optionally data) to a new table",
		Args:  cobra.ExactArgs(2),
		RunE: func(cmd *cobra.Command, args []string) error {
			src, dst := args[0], args[1]
			ctx := context.Background()
			conn := getConn(ctx, database)
			defer conn.Close(ctx)

			var exists bool
			conn.QueryRow(ctx,
				"SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_schema=$1 AND table_name=$2)",
				schema, dst).Scan(&exists)
			if exists {
				fmt.Fprintf(os.Stderr, "Table '%s.%s' already exists. Nothing to do.\n", schema, dst)
				return nil
			}

			q := fmt.Sprintf("CREATE TABLE %s AS TABLE %s", qualifiedTable(schema, dst), qualifiedTable(schema, src))
			if !withData {
				q += " WITH NO DATA"
			}
			if _, err := conn.Exec(ctx, q); err != nil {
				return err
			}
			label := "with data"
			if !withData {
				label = "structure only"
			}
			fmt.Fprintf(os.Stderr, "Table '%s.%s' copied to '%s.%s' (%s).\n", schema, src, schema, dst, label)
			return nil
		},
	}
	cmd.Flags().StringVarP(&database, "database", "d", cfg.Database, "Database name")
	cmd.Flags().StringVarP(&schema, "schema", "s", "public", "Schema name")
	cmd.Flags().BoolVar(&withData, "data", true, "Copy data as well")
	return cmd
}

// ==================== col (column) ====================

func colCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "col",
		Short: "Commands to add, drop, and rename columns on a table",
	}
	cmd.AddCommand(colListCmd(), colAddCmd(), colDropCmd(), colRenameCmd())
	return cmd
}

func colListCmd() *cobra.Command {
	var database, schema string
	cmd := &cobra.Command{
		Use:   "list TABLE",
		Short: "List all columns of a table",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			table := args[0]
			ctx := context.Background()
			conn := getConn(ctx, database)
			defer conn.Close(ctx)

			rows, err := conn.Query(ctx, `
				SELECT column_name, data_type, is_nullable, column_default
				FROM information_schema.columns
				WHERE table_schema = $1 AND table_name = $2
				ORDER BY ordinal_position`, schema, table)
			if err != nil {
				return err
			}
			results, _ := collectRows(rows)
			if len(results) > 0 {
				fmt.Fprintf(os.Stderr, "Columns in %s.%s:\n", schema, table)
				for _, r := range results {
					nullable := "not null"
					if fmt.Sprintf("%v", r["is_nullable"]) == "YES" {
						nullable = "nullable"
					}
					def := ""
					if r["column_default"] != nil {
						def = fmt.Sprintf(" default=%v", r["column_default"])
					}
					fmt.Fprintf(os.Stderr, "  • %s (%s, %s%s)\n", r["column_name"], r["data_type"], nullable, def)
				}
			} else {
				fmt.Fprintf(os.Stderr, "No columns found for %s.%s.\n", schema, table)
			}
			return nil
		},
	}
	cmd.Flags().StringVarP(&database, "database", "d", cfg.Database, "Database name")
	cmd.Flags().StringVarP(&schema, "schema", "s", "public", "Schema name")
	return cmd
}

func colAddCmd() *cobra.Command {
	var database, schema string
	var nullable, isFloat, isDouble, isLong, isUnsigned bool
	var fixed int
	var defaultValue string

	cmd := &cobra.Command{
		Use:   "add TABLE COL_NAME COL_TYPE",
		Short: "Add a column to a table (types: string, number, bool, jsonb)",
		Args:  cobra.ExactArgs(3),
		RunE: func(cmd *cobra.Command, args []string) error {
			table, colName, colType := args[0], args[1], args[2]

			validTypes := map[string]bool{"string": true, "number": true, "bool": true, "jsonb": true}
			if !validTypes[colType] {
				fmt.Fprintf(os.Stderr, "Error: Unknown type '%s'. Use: string, number, bool, jsonb\n", colType)
				os.Exit(1)
			}

			fixedVal := -1
			if cmd.Flags().Changed("fixed") {
				fixedVal = fixed
			}
			pgType := resolveColumnType(colType, isFloat, isDouble, isLong, isUnsigned, fixedVal)

			ctx := context.Background()
			conn := getConn(ctx, database)
			defer conn.Close(ctx)

			nullClause := " NOT NULL"
			if nullable {
				nullClause = ""
			}
			defaultClause := ""
			if defaultValue != "" {
				defaultClause = " DEFAULT " + defaultValue
			}

			q := fmt.Sprintf("ALTER TABLE %s ADD COLUMN IF NOT EXISTS %s %s%s%s",
				qualifiedTable(schema, table), quoteIdent(colName), pgType, nullClause, defaultClause)
			if _, err := conn.Exec(ctx, q); err != nil {
				return err
			}

			if isUnsigned && colType == "number" {
				constraintName := colName + "_unsigned"
				var constraintExists bool
				conn.QueryRow(ctx,
					"SELECT EXISTS(SELECT 1 FROM information_schema.table_constraints WHERE table_schema=$1 AND table_name=$2 AND constraint_name=$3)",
					schema, table, constraintName).Scan(&constraintExists)
				if !constraintExists {
					cq := fmt.Sprintf("ALTER TABLE %s ADD CONSTRAINT %s CHECK (%s >= 0)",
						qualifiedTable(schema, table), quoteIdent(constraintName), quoteIdent(colName))
					if _, err := conn.Exec(ctx, cq); err != nil {
						return err
					}
				}
			}

			label := strings.ToLower(pgType)
			if isUnsigned {
				label = "unsigned " + label
			}
			if nullable {
				label += " (nullable)"
			}
			fmt.Fprintf(os.Stderr, "Column '%s' (%s) added to '%s.%s'.\n", colName, label, schema, table)
			return nil
		},
	}
	cmd.Flags().StringVarP(&database, "database", "d", cfg.Database, "Database name")
	cmd.Flags().StringVarP(&schema, "schema", "s", "public", "Schema name")
	cmd.Flags().BoolVar(&nullable, "nullable", false, "Allow NULL values")
	cmd.Flags().BoolVar(&isFloat, "float", false, "Use 4-byte float (REAL)")
	cmd.Flags().BoolVar(&isDouble, "double", false, "Use 8-byte float (DOUBLE PRECISION)")
	cmd.Flags().BoolVar(&isLong, "long", false, "Use 8-byte integer (BIGINT)")
	cmd.Flags().BoolVar(&isUnsigned, "unsigned", false, "Adds CHECK >= 0 constraint")
	cmd.Flags().IntVar(&fixed, "fixed", -1, "Fixed-point decimal with N fractional digits")
	cmd.Flags().StringVar(&defaultValue, "default", "", "Default value (SQL literal)")
	return cmd
}

func colDropCmd() *cobra.Command {
	var database, schema string
	cmd := &cobra.Command{
		Use:   "drop TABLE COL_NAME",
		Short: "Drop a column from a table",
		Args:  cobra.ExactArgs(2),
		RunE: func(cmd *cobra.Command, args []string) error {
			table, colName := args[0], args[1]
			ctx := context.Background()
			conn := getConn(ctx, database)
			defer conn.Close(ctx)

			q := fmt.Sprintf("ALTER TABLE %s DROP COLUMN IF EXISTS %s",
				qualifiedTable(schema, table), quoteIdent(colName))
			if _, err := conn.Exec(ctx, q); err != nil {
				return err
			}
			fmt.Fprintf(os.Stderr, "Column '%s' dropped from '%s.%s'.\n", colName, schema, table)
			return nil
		},
	}
	cmd.Flags().StringVarP(&database, "database", "d", cfg.Database, "Database name")
	cmd.Flags().StringVarP(&schema, "schema", "s", "public", "Schema name")
	return cmd
}

func colRenameCmd() *cobra.Command {
	var database, schema string
	cmd := &cobra.Command{
		Use:   "rename TABLE OLD_NAME NEW_NAME",
		Short: "Rename a column on a table",
		Args:  cobra.ExactArgs(3),
		RunE: func(cmd *cobra.Command, args []string) error {
			table, oldName, newName := args[0], args[1], args[2]
			ctx := context.Background()
			conn := getConn(ctx, database)
			defer conn.Close(ctx)

			// Check if already in desired state
			rows, err := conn.Query(ctx,
				"SELECT column_name FROM information_schema.columns WHERE table_schema=$1 AND table_name=$2 AND column_name IN ($3, $4)",
				schema, table, oldName, newName)
			if err != nil {
				return err
			}
			existing := map[string]bool{}
			results, _ := collectRows(rows)
			for _, r := range results {
				existing[fmt.Sprintf("%v", r["column_name"])] = true
			}
			if existing[newName] && !existing[oldName] {
				fmt.Fprintf(os.Stderr, "Column '%s' already exists on '%s.%s'. Nothing to do.\n", newName, schema, table)
				return nil
			}

			q := fmt.Sprintf("ALTER TABLE %s RENAME COLUMN %s TO %s",
				qualifiedTable(schema, table), quoteIdent(oldName), quoteIdent(newName))
			if _, err := conn.Exec(ctx, q); err != nil {
				return err
			}
			fmt.Fprintf(os.Stderr, "Column '%s' renamed to '%s' on '%s.%s'.\n", oldName, newName, schema, table)
			return nil
		},
	}
	cmd.Flags().StringVarP(&database, "database", "d", cfg.Database, "Database name")
	cmd.Flags().StringVarP(&schema, "schema", "s", "public", "Schema name")
	return cmd
}

// ==================== idx (index) ====================

func idxCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "idx",
		Short: "Commands to add, drop, and list indexes on a table",
	}
	cmd.AddCommand(idxListCmd(), idxAddCmd(), idxDropCmd())
	return cmd
}

func idxListCmd() *cobra.Command {
	var database, schema string
	cmd := &cobra.Command{
		Use:   "list TABLE",
		Short: "List all indexes on a table",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			table := args[0]
			ctx := context.Background()
			conn := getConn(ctx, database)
			defer conn.Close(ctx)

			rows, err := conn.Query(ctx,
				"SELECT indexname, indexdef FROM pg_indexes WHERE schemaname=$1 AND tablename=$2 ORDER BY indexname",
				schema, table)
			if err != nil {
				return err
			}
			results, _ := collectRows(rows)
			if len(results) > 0 {
				fmt.Fprintf(os.Stderr, "Indexes on %s.%s:\n", schema, table)
				for _, r := range results {
					fmt.Fprintf(os.Stderr, "  • %s\n", r["indexname"])
					fmt.Fprintf(os.Stderr, "    %s\n", r["indexdef"])
				}
			} else {
				fmt.Fprintf(os.Stderr, "No indexes found on %s.%s.\n", schema, table)
			}
			return nil
		},
	}
	cmd.Flags().StringVarP(&database, "database", "d", cfg.Database, "Database name")
	cmd.Flags().StringVarP(&schema, "schema", "s", "public", "Schema name")
	return cmd
}

func idxAddCmd() *cobra.Command {
	var database, schema, name string
	var gin, noGin, isUnique bool

	cmd := &cobra.Command{
		Use:   "add TABLE COLUMNS...",
		Short: "Add an index or unique constraint to a table",
		Long: `Add an index or unique constraint to a table.

For JSONB columns, GIN is used automatically unless --no-gin is passed.

Examples:
  pg idx add resources kind                              # btree index on kind
  pg idx add resources kind namespace                    # composite btree
  pg idx add resources labels                            # GIN (auto-detected)
  pg idx add resources api_version kind name --unique    # unique constraint`,
		Args: cobra.MinimumNArgs(2),
		RunE: func(cmd *cobra.Command, args []string) error {
			table := args[0]
			columns := args[1:]
			ctx := context.Background()
			conn := getConn(ctx, database)
			defer conn.Close(ctx)

			useGin := false
			if !isUnique && !noGin {
				if gin {
					useGin = true
				} else {
					// Auto-detect from column types
					rows, err := conn.Query(ctx, `
						SELECT column_name, udt_name
						FROM information_schema.columns
						WHERE table_schema=$1 AND table_name=$2 AND column_name=ANY($3)`,
						schema, table, columns)
					if err != nil {
						return err
					}
					colTypes := map[string]string{}
					results, _ := collectRows(rows)
					for _, r := range results {
						colTypes[fmt.Sprintf("%v", r["column_name"])] = fmt.Sprintf("%v", r["udt_name"])
					}
					allJsonb := len(columns) > 0
					for _, c := range columns {
						if colTypes[c] != "jsonb" {
							allJsonb = false
							break
						}
					}
					useGin = allJsonb
				}
			}

			if name == "" {
				if isUnique {
					name = table + "_unique_" + strings.Join(columns, "_")
				} else {
					name = "idx_" + table + "_" + strings.Join(columns, "_")
				}
			}

			colIds := make([]string, len(columns))
			for i, c := range columns {
				colIds[i] = quoteIdent(c)
			}
			colsStr := strings.Join(colIds, ", ")

			if isUnique {
				var exists bool
				conn.QueryRow(ctx, "SELECT EXISTS(SELECT 1 FROM pg_constraint WHERE conname=$1)", name).Scan(&exists)
				if exists {
					fmt.Fprintf(os.Stderr, "Unique constraint '%s' already exists on '%s.%s', skipping.\n", name, schema, table)
					return nil
				}
				q := fmt.Sprintf("ALTER TABLE %s ADD CONSTRAINT %s UNIQUE (%s)",
					qualifiedTable(schema, table), quoteIdent(name), colsStr)
				if _, err := conn.Exec(ctx, q); err != nil {
					return err
				}
				fmt.Fprintf(os.Stderr, "Unique constraint '%s' added to '%s.%s' on (%s).\n", name, schema, table, strings.Join(columns, ", "))
			} else {
				using := ""
				if useGin {
					using = "USING gin "
				}
				q := fmt.Sprintf("CREATE INDEX IF NOT EXISTS %s ON %s %s(%s)",
					quoteIdent(name), qualifiedTable(schema, table), using, colsStr)
				if _, err := conn.Exec(ctx, q); err != nil {
					return err
				}
				idxType := "Index"
				if useGin {
					idxType = "GIN index"
				}
				fmt.Fprintf(os.Stderr, "%s '%s' created on '%s.%s' (%s).\n", idxType, name, schema, table, strings.Join(columns, ", "))
			}
			return nil
		},
	}
	cmd.Flags().StringVarP(&database, "database", "d", cfg.Database, "Database name")
	cmd.Flags().StringVarP(&schema, "schema", "s", "public", "Schema name")
	cmd.Flags().StringVarP(&name, "name", "n", "", "Custom index/constraint name")
	cmd.Flags().BoolVar(&gin, "gin", false, "Force GIN index")
	cmd.Flags().BoolVar(&noGin, "no-gin", false, "Force no GIN index")
	cmd.Flags().BoolVar(&isUnique, "unique", false, "Create a UNIQUE constraint")
	return cmd
}

func idxDropCmd() *cobra.Command {
	var database, schema string
	cmd := &cobra.Command{
		Use:   "drop NAME",
		Short: "Drop an index by name",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			name := args[0]
			ctx := context.Background()
			conn := getConn(ctx, database)
			defer conn.Close(ctx)

			q := fmt.Sprintf("DROP INDEX IF EXISTS %s.%s", quoteIdent(schema), quoteIdent(name))
			if _, err := conn.Exec(ctx, q); err != nil {
				return err
			}
			fmt.Fprintf(os.Stderr, "Index '%s' dropped.\n", name)
			return nil
		},
	}
	cmd.Flags().StringVarP(&database, "database", "d", cfg.Database, "Database name")
	cmd.Flags().StringVarP(&schema, "schema", "s", "public", "Schema name")
	return cmd
}

// ==================== row ====================

func rowCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "row",
		Short: "Commands to operate on table rows",
	}
	cmd.AddCommand(rowRmCmd(), rowListCmd())
	return cmd
}

func rowRmCmd() *cobra.Command {
	var database, schema string
	var allRows bool

	cmd := &cobra.Command{
		Use:   "rm TABLE [ID]",
		Short: "Remove a row by id from a table, or all rows with --all",
		Args:  cobra.RangeArgs(1, 2),
		RunE: func(cmd *cobra.Command, args []string) error {
			table := args[0]
			var idStr string
			if len(args) > 1 {
				idStr = args[1]
			}

			if idStr == "" && !allRows {
				fmt.Fprintln(os.Stderr, "Error: provide an id or use --all to delete all rows.")
				os.Exit(1)
			}
			if idStr != "" && allRows {
				fmt.Fprintln(os.Stderr, "Error: cannot specify both an id and --all.")
				os.Exit(1)
			}

			ctx := context.Background()
			conn := getConn(ctx, database)
			defer conn.Close(ctx)

			var tag pgconn.CommandTag
			var err error
			if allRows {
				tag, err = conn.Exec(ctx, fmt.Sprintf("DELETE FROM %s", qualifiedTable(schema, table)))
			} else {
				tag, err = conn.Exec(ctx, fmt.Sprintf("DELETE FROM %s WHERE id = $1", qualifiedTable(schema, table)), idStr)
			}
			if err != nil {
				return err
			}
			if allRows {
				fmt.Fprintf(os.Stderr, "Deleted %d row(s) from '%s.%s'.\n", tag.RowsAffected(), schema, table)
			} else {
				fmt.Fprintf(os.Stderr, "Deleted %d row(s) from '%s.%s' where id=%s.\n", tag.RowsAffected(), schema, table, idStr)
			}
			return nil
		},
	}
	cmd.Flags().StringVarP(&database, "database", "d", cfg.Database, "Database name")
	cmd.Flags().StringVarP(&schema, "schema", "s", "public", "Schema name")
	cmd.Flags().BoolVar(&allRows, "all", false, "Delete all rows in the table")
	return cmd
}

func rowListCmd() *cobra.Command {
	var database, schema string
	var limit int

	cmd := &cobra.Command{
		Use:   "list TABLE",
		Short: "List rows of a table",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			table := args[0]
			ctx := context.Background()
			conn := getConn(ctx, database)
			defer conn.Close(ctx)

			q := fmt.Sprintf("SELECT * FROM %s LIMIT $1", qualifiedTable(schema, table))
			rows, err := conn.Query(ctx, q, limit)
			if err != nil {
				return err
			}
			results, err := collectRows(rows)
			if err != nil {
				return err
			}
			fmt.Println(jsonSerialize(results))
			return nil
		},
	}
	cmd.Flags().StringVarP(&database, "database", "d", cfg.Database, "Database name")
	cmd.Flags().StringVarP(&schema, "schema", "s", "public", "Schema name")
	cmd.Flags().IntVarP(&limit, "limit", "l", 100, "Maximum rows to return")
	return cmd
}

// ==================== schema ====================

func schemaCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "schema [DBNAME]",
		Short: "List all schemas in a database",
		Args:  cobra.MaximumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			dbname := cfg.Database
			if len(args) > 0 {
				dbname = args[0]
			}
			ctx := context.Background()
			conn := getConn(ctx, dbname)
			defer conn.Close(ctx)

			rows, err := conn.Query(ctx, "SELECT schema_name, schema_owner FROM information_schema.schemata ORDER BY schema_name")
			if err != nil {
				return err
			}
			results, _ := collectRows(rows)
			if len(results) > 0 {
				fmt.Fprintf(os.Stderr, "Schemas in %s:\n", dbname)
				for _, r := range results {
					fmt.Fprintf(os.Stderr, "  • %s (owner: %s)\n", r["schema_name"], r["schema_owner"])
				}
			} else {
				fmt.Fprintln(os.Stderr, "No schemas found.")
			}
			return nil
		},
	}
	return cmd
}

// ==================== dump ====================

func dumpCmd() *cobra.Command {
	var database, schema string

	cmd := &cobra.Command{
		Use:   "dump",
		Short: "Dump SQL DDL statements to recreate database schema and tables (no data)",
		RunE: func(cmd *cobra.Command, args []string) error {
			ctx := context.Background()
			conn := getConn(ctx, database)
			defer conn.Close(ctx)

			var out []string
			out = append(out, fmt.Sprintf("-- PostgreSQL schema dump for %s.%s", database, schema))
			out = append(out, "-- Generated by pg")
			out = append(out, "")

			if schema != "public" {
				out = append(out, fmt.Sprintf("CREATE SCHEMA IF NOT EXISTS %s;", schema))
				out = append(out, "")
			}

			// Sequences
			seqRows, err := conn.Query(ctx, `
				SELECT sequence_name, data_type, start_value, minimum_value,
				       maximum_value, increment, cycle_option
				FROM information_schema.sequences
				WHERE sequence_schema = $1
				ORDER BY sequence_name`, schema)
			if err != nil {
				return err
			}
			sequences, _ := collectRows(seqRows)
			if len(sequences) > 0 {
				out = append(out, "-- Sequences")
				for _, seq := range sequences {
					cycle := "NO CYCLE"
					if fmt.Sprintf("%v", seq["cycle_option"]) == "YES" {
						cycle = "CYCLE"
					}
					out = append(out, fmt.Sprintf("CREATE SEQUENCE %s.%s", schema, seq["sequence_name"]))
					out = append(out, fmt.Sprintf("    INCREMENT BY %v", seq["increment"]))
					out = append(out, fmt.Sprintf("    MINVALUE %v", seq["minimum_value"]))
					out = append(out, fmt.Sprintf("    MAXVALUE %v", seq["maximum_value"]))
					out = append(out, fmt.Sprintf("    START WITH %v", seq["start_value"]))
					out = append(out, fmt.Sprintf("    %s;", cycle))
					out = append(out, "")
				}
			}

			// Tables
			tblRows, err := conn.Query(ctx,
				"SELECT tablename FROM pg_tables WHERE schemaname=$1 ORDER BY tablename", schema)
			if err != nil {
				return err
			}
			tables, _ := collectRows(tblRows)
			if len(tables) > 0 {
				out = append(out, "-- Tables")
				out = append(out, "")
			}

			for _, tblRow := range tables {
				tableName := fmt.Sprintf("%v", tblRow["tablename"])
				out = append(out, fmt.Sprintf("CREATE TABLE %s.%s (", schema, tableName))

				// Columns
				colRows, err := conn.Query(ctx, `
					SELECT column_name, data_type, character_maximum_length,
					       numeric_precision, numeric_scale, is_nullable,
					       column_default, udt_name
					FROM information_schema.columns
					WHERE table_schema=$1 AND table_name=$2
					ORDER BY ordinal_position`, schema, tableName)
				if err != nil {
					return err
				}
				columns, _ := collectRows(colRows)

				var colDefs []string
				for _, col := range columns {
					colName := fmt.Sprintf("%v", col["column_name"])
					dataType := fmt.Sprintf("%v", col["data_type"])
					udtName := fmt.Sprintf("%v", col["udt_name"])

					var typeStr string
					switch dataType {
					case "ARRAY":
						baseType := strings.TrimLeft(udtName, "_")
						typeStr = baseType + "[]"
					case "USER-DEFINED":
						typeStr = udtName
					case "character varying", "varchar":
						if col["character_maximum_length"] != nil {
							typeStr = fmt.Sprintf("varchar(%v)", col["character_maximum_length"])
						} else {
							typeStr = "varchar"
						}
					case "character":
						if col["character_maximum_length"] != nil {
							typeStr = fmt.Sprintf("char(%v)", col["character_maximum_length"])
						} else {
							typeStr = "char"
						}
					case "numeric":
						if col["numeric_precision"] != nil && col["numeric_scale"] != nil {
							typeStr = fmt.Sprintf("numeric(%v,%v)", col["numeric_precision"], col["numeric_scale"])
						} else if col["numeric_precision"] != nil {
							typeStr = fmt.Sprintf("numeric(%v)", col["numeric_precision"])
						} else {
							typeStr = "numeric"
						}
					default:
						typeStr = dataType
					}

					def := fmt.Sprintf("    %s %s", colName, typeStr)
					if col["column_default"] != nil {
						def += fmt.Sprintf(" DEFAULT %v", col["column_default"])
					}
					if fmt.Sprintf("%v", col["is_nullable"]) == "NO" {
						def += " NOT NULL"
					}
					colDefs = append(colDefs, def)
				}

				// Primary key
				pkRows, err := conn.Query(ctx, `
					SELECT kcu.column_name
					FROM information_schema.table_constraints tc
					JOIN information_schema.key_column_usage kcu
					    ON tc.constraint_name = kcu.constraint_name AND tc.table_schema = kcu.table_schema
					WHERE tc.constraint_type = 'PRIMARY KEY'
					    AND tc.table_schema = $1 AND tc.table_name = $2
					ORDER BY kcu.ordinal_position`, schema, tableName)
				if err != nil {
					return err
				}
				pkResults, _ := collectRows(pkRows)
				if len(pkResults) > 0 {
					pkCols := make([]string, len(pkResults))
					for i, r := range pkResults {
						pkCols[i] = fmt.Sprintf("%v", r["column_name"])
					}
					colDefs = append(colDefs, fmt.Sprintf("    PRIMARY KEY (%s)", strings.Join(pkCols, ", ")))
				}

				// Unique constraints
				ucRows, err := conn.Query(ctx, `
					SELECT tc.constraint_name, array_agg(kcu.column_name ORDER BY kcu.ordinal_position) as columns
					FROM information_schema.table_constraints tc
					JOIN information_schema.key_column_usage kcu
					    ON tc.constraint_name = kcu.constraint_name AND tc.table_schema = kcu.table_schema
					WHERE tc.constraint_type = 'UNIQUE'
					    AND tc.table_schema = $1 AND tc.table_name = $2
					GROUP BY tc.constraint_name`, schema, tableName)
				if err != nil {
					return err
				}
				ucResults, _ := collectRows(ucRows)
				for _, uc := range ucResults {
					cols := fmt.Sprintf("%v", uc["columns"])
					// pgx returns []string for array_agg
					if arr, ok := uc["columns"].([]any); ok {
						parts := make([]string, len(arr))
						for i, v := range arr {
							parts[i] = fmt.Sprintf("%v", v)
						}
						cols = strings.Join(parts, ", ")
					}
					colDefs = append(colDefs, fmt.Sprintf("    UNIQUE (%s)", cols))
				}

				// Foreign keys
				fkRows, err := conn.Query(ctx, `
					SELECT tc.constraint_name, kcu.column_name,
					       ccu.table_schema AS foreign_table_schema,
					       ccu.table_name AS foreign_table_name,
					       ccu.column_name AS foreign_column_name,
					       rc.update_rule, rc.delete_rule
					FROM information_schema.table_constraints tc
					JOIN information_schema.key_column_usage kcu
					    ON tc.constraint_name = kcu.constraint_name AND tc.table_schema = kcu.table_schema
					JOIN information_schema.constraint_column_usage ccu
					    ON ccu.constraint_name = tc.constraint_name AND ccu.table_schema = tc.table_schema
					JOIN information_schema.referential_constraints rc
					    ON rc.constraint_name = tc.constraint_name AND rc.constraint_schema = tc.table_schema
					WHERE tc.constraint_type = 'FOREIGN KEY'
					    AND tc.table_schema = $1 AND tc.table_name = $2`, schema, tableName)
				if err != nil {
					return err
				}
				fkResults, _ := collectRows(fkRows)
				for _, fk := range fkResults {
					fkDef := fmt.Sprintf("    FOREIGN KEY (%v) REFERENCES %v.%v(%v)",
						fk["column_name"], fk["foreign_table_schema"], fk["foreign_table_name"], fk["foreign_column_name"])
					if fmt.Sprintf("%v", fk["delete_rule"]) != "NO ACTION" {
						fkDef += fmt.Sprintf(" ON DELETE %v", fk["delete_rule"])
					}
					if fmt.Sprintf("%v", fk["update_rule"]) != "NO ACTION" {
						fkDef += fmt.Sprintf(" ON UPDATE %v", fk["update_rule"])
					}
					colDefs = append(colDefs, fkDef)
				}

				// Check constraints
				ckRows, err := conn.Query(ctx, `
					SELECT cc.constraint_name, cc.check_clause
					FROM information_schema.check_constraints cc
					JOIN information_schema.table_constraints tc
					    ON cc.constraint_name = tc.constraint_name AND cc.constraint_schema = tc.table_schema
					WHERE tc.table_schema = $1 AND tc.table_name = $2
					    AND tc.constraint_type = 'CHECK'
					    AND cc.constraint_name NOT LIKE '%_not_null'`, schema, tableName)
				if err != nil {
					return err
				}
				ckResults, _ := collectRows(ckRows)
				for _, ck := range ckResults {
					colDefs = append(colDefs, fmt.Sprintf("    CHECK (%v)", ck["check_clause"]))
				}

				out = append(out, strings.Join(colDefs, ",\n"))
				out = append(out, ");")
				out = append(out, "")
			}

			// Indexes (non-PK, non-unique)
			idxRows, err := conn.Query(ctx, `
				SELECT indexname, indexdef
				FROM pg_indexes
				WHERE schemaname = $1
				    AND indexname NOT IN (
				        SELECT constraint_name
				        FROM information_schema.table_constraints
				        WHERE table_schema = $1
				            AND constraint_type IN ('PRIMARY KEY', 'UNIQUE')
				    )
				ORDER BY tablename, indexname`, schema)
			if err != nil {
				return err
			}
			indexes, _ := collectRows(idxRows)
			if len(indexes) > 0 {
				out = append(out, "-- Indexes")
				for _, idx := range indexes {
					out = append(out, fmt.Sprintf("%v;", idx["indexdef"]))
				}
				out = append(out, "")
			}

			fmt.Println(strings.Join(out, "\n"))
			return nil
		},
	}
	cmd.Flags().StringVarP(&database, "database", "d", cfg.Database, "Database name")
	cmd.Flags().StringVarP(&schema, "schema", "s", "public", "Schema name")
	return cmd
}

// ==================== query ====================

func runQueryFromReader(r io.Reader, database, outputFormat string) error {
	data, err := io.ReadAll(r)
	if err != nil {
		return err
	}
	return execSQL(string(data), database, outputFormat)
}

func execSQL(sql, database, outputFormat string) error {
	ctx := context.Background()
	conn := getConn(ctx, database)
	defer conn.Close(ctx)

	rows, err := conn.Query(ctx, sql)
	if err != nil {
		// Could be a non-SELECT statement
		tag, execErr := conn.Exec(ctx, sql)
		if execErr != nil {
			j, _ := json.Marshal(map[string]string{"error": execErr.Error()})
			fmt.Fprintln(os.Stderr, string(j))
			os.Exit(1)
		}
		j, _ := json.MarshalIndent(map[string]any{"status": "success", "rows_affected": tag.RowsAffected()}, "", "  ")
		fmt.Println(string(j))
		return nil
	}

	results, err := collectRows(rows)
	if err != nil {
		j, _ := json.Marshal(map[string]string{"error": err.Error()})
		fmt.Fprintln(os.Stderr, string(j))
		os.Exit(1)
	}

	if outputFormat == "table" {
		printResults(results)
	} else {
		fmt.Println(jsonSerialize(results))
	}
	return nil
}

func queryCmd() *cobra.Command {
	var database, outputFormat string

	cmd := &cobra.Command{
		Use:   "query [SQL_PATH]",
		Short: "Execute a SQL file, directory of SQL files, or SQL from stdin",
		Args:  cobra.MaximumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			if len(args) == 0 {
				fi, _ := os.Stdin.Stat()
				if (fi.Mode() & os.ModeCharDevice) != 0 {
					fmt.Fprintln(os.Stderr, "Error: No SQL path provided and no input on stdin.")
					os.Exit(1)
				}
				return runQueryFromReader(os.Stdin, database, outputFormat)
			}

			sqlPath := args[0]
			info, err := os.Stat(sqlPath)
			if err != nil {
				return fmt.Errorf("path not found: %s", sqlPath)
			}

			if info.IsDir() {
				// Directory mode
				ctx := context.Background()
				conn := getConn(ctx, database)
				defer conn.Close(ctx)

				entries, _ := os.ReadDir(sqlPath)
				var sqlFiles []string
				for _, e := range entries {
					if strings.HasSuffix(e.Name(), ".sql") {
						sqlFiles = append(sqlFiles, e.Name())
					}
				}
				sort.Strings(sqlFiles)

				results := make(map[string]any)
				for _, f := range sqlFiles {
					data, err := os.ReadFile(filepath.Join(sqlPath, f))
					if err != nil {
						results[f] = map[string]string{"error": err.Error()}
						continue
					}
					rows, err := conn.Query(ctx, string(data))
					if err != nil {
						// Try as non-SELECT
						tag, execErr := conn.Exec(ctx, string(data))
						if execErr != nil {
							results[f] = map[string]string{"error": execErr.Error()}
						} else {
							results[f] = map[string]any{"status": "success", "rows_affected": tag.RowsAffected()}
						}
						continue
					}
					collected, err := collectRows(rows)
					if err != nil {
						results[f] = map[string]string{"error": err.Error()}
					} else {
						results[f] = collected
					}
				}
				j, _ := json.MarshalIndent(results, "", "  ")
				fmt.Println(string(j))
				return nil
			}

			// Single file
			data, err := os.ReadFile(sqlPath)
			if err != nil {
				return err
			}
			return execSQL(string(data), database, outputFormat)
		},
	}
	cmd.Flags().StringVarP(&database, "database", "d", "", "Database name")
	cmd.Flags().StringVarP(&outputFormat, "format", "f", "json", "Output format (json or table)")
	return cmd
}

// ==================== db ====================

func dbCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "db",
		Short: "Commands related to databases",
	}
	cmd.AddCommand(dbListCmd(), dbAddCmd(), dbDropCmd(), dbSaveCmd(), dbRestoreCmd())
	return cmd
}

func dbListCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "list",
		Short: "List all databases",
		RunE: func(cmd *cobra.Command, args []string) error {
			ctx := context.Background()
			conn := getConn(ctx, "")
			defer conn.Close(ctx)

			rows, err := conn.Query(ctx, `
				SELECT datname, pg_catalog.pg_get_userbyid(datdba) as owner,
				       pg_catalog.pg_encoding_to_char(encoding) as encoding
				FROM pg_catalog.pg_database
				WHERE datistemplate = false
				ORDER BY datname`)
			if err != nil {
				return err
			}
			results, _ := collectRows(rows)
			if len(results) > 0 {
				fmt.Fprintln(os.Stderr, "Databases:")
				for _, r := range results {
					fmt.Fprintf(os.Stderr, "  • %s (owner: %s, encoding: %s)\n", r["datname"], r["owner"], r["encoding"])
				}
			} else {
				fmt.Fprintln(os.Stderr, "No databases found.")
			}
			return nil
		},
	}
}

func dbAddCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "add NAME",
		Short: "Create a new database",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			name := args[0]
			ctx := context.Background()
			conn := getConn(ctx, "postgres")
			defer conn.Close(ctx)

			var exists bool
			conn.QueryRow(ctx, "SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname=$1)", name).Scan(&exists)
			if exists {
				fmt.Fprintf(os.Stderr, "Database '%s' already exists.\n", name)
				return nil
			}

			// CREATE DATABASE cannot run inside a transaction
			_, err := conn.Exec(ctx, fmt.Sprintf("CREATE DATABASE %s", quoteIdent(name)))
			if err != nil {
				return err
			}
			fmt.Fprintf(os.Stderr, "Database '%s' created successfully.\n", name)
			return nil
		},
	}
}

func dbDropCmd() *cobra.Command {
	var force bool
	cmd := &cobra.Command{
		Use:   "drop NAME",
		Short: "Drop a database",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			name := args[0]
			ctx := context.Background()
			conn := getConn(ctx, "postgres")
			defer conn.Close(ctx)

			if force {
				conn.Exec(ctx,
					"SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname=$1 AND pid <> pg_backend_pid()", name)
			}
			_, err := conn.Exec(ctx, fmt.Sprintf("DROP DATABASE IF EXISTS %s", quoteIdent(name)))
			if err != nil {
				return err
			}
			fmt.Fprintf(os.Stderr, "Database '%s' dropped.\n", name)
			return nil
		},
	}
	cmd.Flags().BoolVar(&force, "force", false, "Terminate active connections before dropping")
	return cmd
}

func dbSaveCmd() *cobra.Command {
	var output string
	cmd := &cobra.Command{
		Use:   "save NAME",
		Short: "Save (dump) a database to a compressed file using pg_dump",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			name := args[0]
			if output == "" {
				output = name + ".sql.gz"
			}

			env := os.Environ()
			env = append(env, fmt.Sprintf("PGPASSWORD=%s", envOr("PGPASSWORD", cfg.Password)))

			dumpArgs := []string{
				"-h", envOr("PGHOST", cfg.Host),
				"-p", envOr("PGPORT", fmt.Sprintf("%d", cfg.Port)),
				"-U", envOr("PGUSER", cfg.User),
				"-d", name,
			}

			f, err := os.Create(output)
			if err != nil {
				return err
			}
			defer f.Close()

			gz := gzip.NewWriter(f)
			defer gz.Close()

			dumpCmd := exec.Command("pg_dump", dumpArgs...)
			dumpCmd.Env = env
			dumpCmd.Stdout = gz
			dumpCmd.Stderr = os.Stderr

			if err := dumpCmd.Run(); err != nil {
				return fmt.Errorf("pg_dump failed: %w", err)
			}
			gz.Close()
			f.Close()

			fmt.Fprintf(os.Stderr, "Database '%s' saved to '%s'.\n", name, output)
			return nil
		},
	}
	cmd.Flags().StringVarP(&output, "output", "o", "", "Output file path (default: <name>.sql.gz)")
	return cmd
}

func dbRestoreCmd() *cobra.Command {
	var name string
	var create bool

	cmd := &cobra.Command{
		Use:   "restore INPUT_FILE",
		Short: "Restore a database from a compressed dump file",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			inputFile := args[0]
			if name == "" {
				base := filepath.Base(inputFile)
				name = strings.SplitN(base, ".", 2)[0]
				if name == "" {
					return fmt.Errorf("could not infer database name from filename; use --name")
				}
			}

			ctx := context.Background()

			if create {
				conn := getConn(ctx, "postgres")
				_, err := conn.Exec(ctx, fmt.Sprintf("CREATE DATABASE %s", quoteIdent(name)))
				conn.Close(ctx)
				if err != nil {
					if strings.Contains(err.Error(), "already exists") {
						fmt.Fprintf(os.Stderr, "Database '%s' already exists, restoring into it.\n", name)
					} else {
						return fmt.Errorf("error creating database: %w", err)
					}
				} else {
					fmt.Fprintf(os.Stderr, "Database '%s' created.\n", name)
				}
			}

			env := os.Environ()
			env = append(env, fmt.Sprintf("PGPASSWORD=%s", envOr("PGPASSWORD", cfg.Password)))

			psqlArgs := []string{
				"-h", envOr("PGHOST", cfg.Host),
				"-p", envOr("PGPORT", fmt.Sprintf("%d", cfg.Port)),
				"-U", envOr("PGUSER", cfg.User),
				"-d", name,
			}

			f, err := os.Open(inputFile)
			if err != nil {
				return err
			}
			defer f.Close()

			gz, err := gzip.NewReader(f)
			if err != nil {
				return fmt.Errorf("failed to read gzip: %w", err)
			}
			defer gz.Close()

			psqlCmd := exec.Command("psql", psqlArgs...)
			psqlCmd.Env = env
			psqlCmd.Stdin = gz
			psqlCmd.Stdout = io.Discard
			psqlCmd.Stderr = os.Stderr

			if err := psqlCmd.Run(); err != nil {
				return fmt.Errorf("psql failed: %w", err)
			}
			fmt.Fprintf(os.Stderr, "Database '%s' restored from '%s'.\n", name, inputFile)
			return nil
		},
	}
	cmd.Flags().StringVarP(&name, "name", "n", "", "Database name (default: inferred from filename)")
	cmd.Flags().BoolVar(&create, "create", true, "Create the database before restoring")
	return cmd
}
