/*
Copyright (C) 2025- Hamed Ghasemzadeh. All rights reserved.
License: HGL General License <https://evgnomon.org/docs/hgl>
*/
package controller

import (
	"context"
	"database/sql"
	"fmt"
	"strings"

	"github.com/evgnomon/zygote/lib/cluster/container"
	"github.com/evgnomon/zygote/lib/cluster/http"
	"github.com/evgnomon/zygote/lib/cluster/tables"
	"github.com/evgnomon/zygote/lib/cluster/utils"
)

const routerReadPort = 6446
const routerWritePort = 6447
const defaultNumShards = 3

var logger = utils.NewLogger()

type SQLQueryRequest struct {
	Query string `json:"query" form:"query"`
}

type SQLQueryController struct {
	connector *tables.MultiDBConnector
}

func NewSQLQueryController() (*SQLQueryController, error) {
	// Initialize database configuration
	ctx := context.Background()
	connector := tables.NewMultiDBConnector(container.AppNetworkName(), "zygote", utils.DomainName(), "mysql",
		routerReadPort, routerWritePort, defaultNumShards)
	_, err := connector.ConnectAllShardsRead(ctx)
	if err != nil {
		return nil, err
	}
	_, err = connector.ConnectAllShardsWrite(ctx)
	if err != nil {
		return nil, err
	}
	dc := &SQLQueryController{
		connector: connector,
	}
	return dc, nil
}

// Close cleans up database resources
func (dc *SQLQueryController) Close() error {
	logger.Debug("Closing database connections")
	return dc.connector.CloseAll()
}

// QueryHandler handles SQL query requests
func (dc *SQLQueryController) QueryHandler(c http.Context) error {
	var req SQLQueryRequest
	err := c.BindBody(&req)
	if err != nil {
		return err
	}

	query := strings.TrimSpace(req.Query)
	if query == "" {
		// return c.SendJSONError(nethttp.StatusBadRequest, map[string]string{
		return c.SendError("Query cannot be empty")
	}

	// Execute query with retry on connection loss
	return dc.connector.RetryReadOperation(c.GetRequestContext(), 0, func(db *sql.DB) error {
		rows, err := db.QueryContext(c.GetRequestContext(), query)
		if err != nil {
			return err
		}
		if rows == nil {
			return c.SendInternalError("Query returned no rows after multiple attempts", err)
		}
		defer rows.Close()

		// Get column names
		columns, err := rows.Columns()
		if err != nil {
			return c.SendInternalError("Failed to get columns: ", err)
		}

		// Process results
		var results []map[string]any
		for rows.Next() {
			values := make([]any, len(columns))
			valuePtrs := make([]any, len(columns))
			for i := range values {
				valuePtrs[i] = &values[i]
			}

			if err := rows.Scan(valuePtrs...); err != nil {
				return c.SendInternalError("Failed to scan row: ", err)
			}

			row := make(map[string]any)
			for i, col := range columns {
				var v any
				if values[i] != nil {
					switch va := values[i].(type) {
					case []byte:
						v = string(va)
					default:
						v = values[i]
					}
				}
				row[col] = v
			}
			results = append(results, row)
		}
		if err = rows.Err(); err != nil {
			return c.SendInternalError("Row iteration error", err)
		}
		return c.Send(results)
	})
}

// ClusterMember defines the structure for cluster member info
type ClusterMember struct {
	MemberID      string `json:"member_id"`
	MemberHost    string `json:"member_host"`
	MemberPort    int    `json:"member_port"`
	MemberState   string `json:"member_state"`
	MemberRole    string `json:"member_role"`
	MemberVersion string `json:"member_version"`
}

// ClusterStatusHandler is a specific handler using GenericQueryHandler
func (dc *SQLQueryController) ClusterStatusHandler(c http.Context) error {
	query := `
        SELECT
            MEMBER_ID,
            MEMBER_HOST,
            MEMBER_PORT,
            MEMBER_STATE,
            MEMBER_ROLE,
            MEMBER_VERSION
        FROM
            performance_schema.replication_group_members
    `
	var results = []ClusterMember{}
	err := dc.connector.GenericQueryHandler(c.GetRequestContext(), 0, query, results, c)
	if err != nil {
		return c.SendInternalError("Failed to execute query: ", err)
	}
	return nil
}

// AddEndpoint configures the controller routes
func (dc *SQLQueryController) AddEndpoint(prefix string, e http.Router) error {
	err := e.Add(http.GET, fmt.Sprintf("%s/sql/cluster/node", prefix), dc.ClusterStatusHandler)
	if err != nil {
		return err
	}
	err = e.Add(http.POST, fmt.Sprintf("%s/sql/query", prefix), dc.QueryHandler)
	if err != nil {
		return err
	}
	return nil
}
