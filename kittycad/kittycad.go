// Package kittycad provides primitives to interact with the openapi HTTP API.
//
// Code generated by github.com/deepmap/oapi-codegen version v1.9.0 DO NOT EDIT.
package kittycad

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"io/ioutil"
	"net/http"
	"net/url"
	"strings"
	"time"

	"github.com/deepmap/oapi-codegen/pkg/runtime"
	openapi_types "github.com/deepmap/oapi-codegen/pkg/types"
)

const (
	BearerAuthScopes = "bearerAuth.Scopes"
)

// Defines values for Environment.
const (
	EnvironmentDEVELOPMENT Environment = "DEVELOPMENT"

	EnvironmentPREVIEW Environment = "PREVIEW"

	EnvironmentPRODUCTION Environment = "PRODUCTION"
)

// Defines values for FileConversionStatus.
const (
	FileConversionStatusCompleted FileConversionStatus = "Completed"

	FileConversionStatusFailed FileConversionStatus = "Failed"

	FileConversionStatusInProgress FileConversionStatus = "In Progress"

	FileConversionStatusQueued FileConversionStatus = "Queued"

	FileConversionStatusUploaded FileConversionStatus = "Uploaded"
)

// Defines values for ValidFileTypes.
const (
	ValidFileTypesDwg ValidFileTypes = "dwg"

	ValidFileTypesDxf ValidFileTypes = "dxf"

	ValidFileTypesObj ValidFileTypes = "obj"

	ValidFileTypesStep ValidFileTypes = "step"

	ValidFileTypesStl ValidFileTypes = "stl"
)

// AuthSession defines model for AuthSession.
type AuthSession struct {
	// The date and time the session/request was created.
	CreatedAt *time.Time `json:"created_at,omitempty"`

	// The user's email address.
	Email *openapi_types.Email `json:"email,omitempty"`

	// The id of the session.
	Id *string `json:"id,omitempty"`

	// The IP address the request originated from.
	IpAddress *string `json:"ip_address,omitempty"`

	// If the token is valid.
	IsValid *bool `json:"is_valid,omitempty"`

	// The user's token.
	Token *string `json:"token,omitempty"`

	// The user's id.
	UserId *string `json:"user_id,omitempty"`
}

// The type of environment.
type Environment string

// ErrorMessage defines model for ErrorMessage.
type ErrorMessage struct {
	// The message.
	Message *string `json:"message,omitempty"`
}

// FileConversion defines model for FileConversion.
type FileConversion struct {
	// The date and time the file conversion was completed.
	CompletedAt *time.Time `json:"completed_at,omitempty"`

	// The date and time the file conversion was created.
	CreatedAt *time.Time `json:"created_at,omitempty"`

	// The id of the file conversion.
	Id *string `json:"id,omitempty"`

	// The converted file, base64 encoded.
	Output       *string         `json:"output,omitempty"`
	OutputFormat *ValidFileTypes `json:"output_format,omitempty"`
	SrcFormat    *ValidFileTypes `json:"src_format,omitempty"`

	// The status of the file conversion.
	Status *FileConversionStatus `json:"status,omitempty"`
}

// The status of the file conversion.
type FileConversionStatus string

// InstanceMetadata defines model for InstanceMetadata.
type InstanceMetadata struct {
	// The CPU platform of the instance.
	CpuPlatform *string `json:"cpu_platform,omitempty"`

	// The description of the instance.
	Description *string `json:"description,omitempty"`

	// The type of environment.
	Environment *Environment `json:"environment,omitempty"`

	// The git hash of the code the server was built from.
	GitHash *string `json:"git_hash,omitempty"`

	// The hostname of the instance.
	Hostname *string `json:"hostname,omitempty"`

	// The id of the instance.
	Id *string `json:"id,omitempty"`

	// The image that was used as the base of the instance.
	Image *string `json:"image,omitempty"`

	// The IP address of the instance.
	IpAddress *string `json:"ip_address,omitempty"`

	// The machine type of the instance.
	MachineType *string `json:"machine_type,omitempty"`

	// The name of the instance.
	Name *string `json:"name,omitempty"`

	// The zone of the instance.
	Zone *string `json:"zone,omitempty"`
}

// Message defines model for Message.
type Message struct {
	// The message.
	Message *string `json:"message,omitempty"`
}

// ValidFileTypes defines model for ValidFileTypes.
type ValidFileTypes string

// BadRequest defines model for BadRequest.
type BadRequest ErrorMessage

// Forbidden defines model for Forbidden.
type Forbidden ErrorMessage

// NotAcceptable defines model for NotAcceptable.
type NotAcceptable ErrorMessage

// NotFound defines model for NotFound.
type NotFound ErrorMessage

// Unauthorized defines model for Unauthorized.
type Unauthorized ErrorMessage

// RequestEditorFn  is the function signature for the RequestEditor callback function
type RequestEditorFn func(ctx context.Context, req *http.Request) error

// Doer performs HTTP requests.
//
// The standard http.Client implements this interface.
type HttpRequestDoer interface {
	Do(req *http.Request) (*http.Response, error)
}

// Client which conforms to the OpenAPI3 specification for this service.
type Client struct {
	// The endpoint of the server conforming to this interface, with scheme,
	// https://api.deepmap.com for example. This can contain a path relative
	// to the server, such as https://api.deepmap.com/dev-test, and all the
	// paths in the swagger spec will be appended to the server.
	Server string

	// Doer for performing requests, typically a *http.Client with any
	// customized settings, such as certificate chains.
	Client HttpRequestDoer

	// A list of callbacks for modifying requests which are generated before sending over
	// the network.
	RequestEditors []RequestEditorFn
}

// ClientOption allows setting custom parameters during construction
type ClientOption func(*Client) error

// Creates a new Client, with reasonable defaults
func newClient(server string, opts ...ClientOption) (*Client, error) {
	// create a client with sane default values
	client := Client{
		Server: server,
	}
	// mutate client and add all optional params
	for _, o := range opts {
		if err := o(&client); err != nil {
			return nil, err
		}
	}
	// ensure the server URL always has a trailing slash
	if !strings.HasSuffix(client.Server, "/") {
		client.Server += "/"
	}
	// create httpClient, if not already present
	if client.Client == nil {
		client.Client = &http.Client{}
	}
	return &client, nil
}

// WithHTTPClient allows overriding the default Doer, which is
// automatically created using http.Client. This is useful for tests.
func WithHTTPClient(doer HttpRequestDoer) ClientOption {
	return func(c *Client) error {
		c.Client = doer
		return nil
	}
}

// WithRequestEditorFn allows setting up a callback function, which will be
// called right before sending the request. This can be used to mutate the request.
func WithRequestEditorFn(fn RequestEditorFn) ClientOption {
	return func(c *Client) error {
		c.RequestEditors = append(c.RequestEditors, fn)
		return nil
	}
}

// newMetaDebugInstanceRequest generates requests for MetaDebugInstance
func newMetaDebugInstanceRequest(server string) (*http.Request, error) {
	var err error

	serverURL, err := url.Parse(server)
	if err != nil {
		return nil, err
	}

	operationPath := fmt.Sprintf("/_meta/debug/instance")
	if operationPath[0] == '/' {
		operationPath = "." + operationPath
	}

	queryURL, err := serverURL.Parse(operationPath)
	if err != nil {
		return nil, err
	}

	req, err := http.NewRequest("GET", queryURL.String(), nil)
	if err != nil {
		return nil, err
	}

	return req, nil
}

// newMetaDebugSessionRequest generates requests for MetaDebugSession
func newMetaDebugSessionRequest(server string) (*http.Request, error) {
	var err error

	serverURL, err := url.Parse(server)
	if err != nil {
		return nil, err
	}

	operationPath := fmt.Sprintf("/_meta/debug/session")
	if operationPath[0] == '/' {
		operationPath = "." + operationPath
	}

	queryURL, err := serverURL.Parse(operationPath)
	if err != nil {
		return nil, err
	}

	req, err := http.NewRequest("GET", queryURL.String(), nil)
	if err != nil {
		return nil, err
	}

	return req, nil
}

// newFileConversionByIDRequest generates requests for FileConversionByID
func newFileConversionByIDRequest(server string, id string) (*http.Request, error) {
	var err error

	var pathParam0 string

	pathParam0, err = runtime.StyleParamWithLocation("simple", false, "id", runtime.ParamLocationPath, id)
	if err != nil {
		return nil, err
	}

	serverURL, err := url.Parse(server)
	if err != nil {
		return nil, err
	}

	operationPath := fmt.Sprintf("/file/conversion/%s", pathParam0)
	if operationPath[0] == '/' {
		operationPath = "." + operationPath
	}

	queryURL, err := serverURL.Parse(operationPath)
	if err != nil {
		return nil, err
	}

	req, err := http.NewRequest("GET", queryURL.String(), nil)
	if err != nil {
		return nil, err
	}

	return req, nil
}

// newFileConvertRequestWithBody generates requests for FileConvert with any type of body
func newFileConvertRequestWithBody(server string, sourceFormat ValidFileTypes, outputFormat ValidFileTypes, contentType string, body io.Reader) (*http.Request, error) {
	var err error

	var pathParam0 string

	pathParam0, err = runtime.StyleParamWithLocation("simple", false, "sourceFormat", runtime.ParamLocationPath, sourceFormat)
	if err != nil {
		return nil, err
	}

	var pathParam1 string

	pathParam1, err = runtime.StyleParamWithLocation("simple", false, "outputFormat", runtime.ParamLocationPath, outputFormat)
	if err != nil {
		return nil, err
	}

	serverURL, err := url.Parse(server)
	if err != nil {
		return nil, err
	}

	operationPath := fmt.Sprintf("/file/conversion/%s/%s", pathParam0, pathParam1)
	if operationPath[0] == '/' {
		operationPath = "." + operationPath
	}

	queryURL, err := serverURL.Parse(operationPath)
	if err != nil {
		return nil, err
	}

	req, err := http.NewRequest("POST", queryURL.String(), body)
	if err != nil {
		return nil, err
	}

	req.Header.Add("Content-Type", contentType)

	return req, nil
}

// newPingRequest generates requests for Ping
func newPingRequest(server string) (*http.Request, error) {
	var err error

	serverURL, err := url.Parse(server)
	if err != nil {
		return nil, err
	}

	operationPath := fmt.Sprintf("/ping")
	if operationPath[0] == '/' {
		operationPath = "." + operationPath
	}

	queryURL, err := serverURL.Parse(operationPath)
	if err != nil {
		return nil, err
	}

	req, err := http.NewRequest("GET", queryURL.String(), nil)
	if err != nil {
		return nil, err
	}

	return req, nil
}

func (c *Client) applyEditors(ctx context.Context, req *http.Request, additionalEditors []RequestEditorFn) error {
	for _, r := range c.RequestEditors {
		if err := r(ctx, req); err != nil {
			return err
		}
	}
	for _, r := range additionalEditors {
		if err := r(ctx, req); err != nil {
			return err
		}
	}
	return nil
}

// HTTPError is an error returned by a failed API call
type HTTPError struct {
	StatusCode int
	RequestURL *url.URL
	Message    string
}

func (err HTTPError) Error() string {
	if msgs := strings.SplitN(err.Message, "\n", 2); len(msgs) > 1 {
		return fmt.Sprintf("HTTP %d: %s (%s)\n%s", err.StatusCode, msgs[0], err.RequestURL, msgs[1])
	} else if err.Message != "" {
		return fmt.Sprintf("HTTP %d: %s (%s)", err.StatusCode, err.Message, err.RequestURL)
	}
	return fmt.Sprintf("HTTP %d (%s)", err.StatusCode, err.RequestURL)
}

// WithBaseURL overrides the baseURL.
func WithBaseURL(baseURL string) ClientOption {
	return func(c *Client) error {
		newBaseURL, err := url.Parse(baseURL)
		if err != nil {
			return err
		}
		c.Server = newBaseURL.String()
		return nil
	}
}

type MetaDebugInstanceResponse struct {
	Body         []byte
	HTTPResponse *http.Response
	JSON200      *InstanceMetadata
	JSON400      *ErrorMessage
	JSON401      *ErrorMessage
	JSON403      *ErrorMessage
}

// Status returns HTTPResponse.Status
func (r MetaDebugInstanceResponse) Status() string {
	if r.HTTPResponse != nil {
		return r.HTTPResponse.Status
	}
	return http.StatusText(0)
}

// StatusCode returns HTTPResponse.StatusCode
func (r MetaDebugInstanceResponse) StatusCode() int {
	if r.HTTPResponse != nil {
		return r.HTTPResponse.StatusCode
	}
	return 0
}

type MetaDebugSessionResponse struct {
	Body         []byte
	HTTPResponse *http.Response
	JSON200      *AuthSession
	JSON400      *ErrorMessage
	JSON401      *ErrorMessage
	JSON403      *ErrorMessage
}

// Status returns HTTPResponse.Status
func (r MetaDebugSessionResponse) Status() string {
	if r.HTTPResponse != nil {
		return r.HTTPResponse.Status
	}
	return http.StatusText(0)
}

// StatusCode returns HTTPResponse.StatusCode
func (r MetaDebugSessionResponse) StatusCode() int {
	if r.HTTPResponse != nil {
		return r.HTTPResponse.StatusCode
	}
	return 0
}

type FileConversionByIDResponse struct {
	Body         []byte
	HTTPResponse *http.Response
	JSON200      *FileConversion
	JSON400      *ErrorMessage
	JSON401      *ErrorMessage
	JSON403      *ErrorMessage
	JSON404      *ErrorMessage
	JSON406      *ErrorMessage
}

// Status returns HTTPResponse.Status
func (r FileConversionByIDResponse) Status() string {
	if r.HTTPResponse != nil {
		return r.HTTPResponse.Status
	}
	return http.StatusText(0)
}

// StatusCode returns HTTPResponse.StatusCode
func (r FileConversionByIDResponse) StatusCode() int {
	if r.HTTPResponse != nil {
		return r.HTTPResponse.StatusCode
	}
	return 0
}

type FileConvertResponse struct {
	Body         []byte
	HTTPResponse *http.Response
	JSON200      *FileConversion
	JSON202      *FileConversion
	JSON400      *ErrorMessage
	JSON401      *ErrorMessage
	JSON403      *ErrorMessage
	JSON406      *ErrorMessage
}

// Status returns HTTPResponse.Status
func (r FileConvertResponse) Status() string {
	if r.HTTPResponse != nil {
		return r.HTTPResponse.Status
	}
	return http.StatusText(0)
}

// StatusCode returns HTTPResponse.StatusCode
func (r FileConvertResponse) StatusCode() int {
	if r.HTTPResponse != nil {
		return r.HTTPResponse.StatusCode
	}
	return 0
}

type PingResponse struct {
	Body         []byte
	HTTPResponse *http.Response
	JSON200      *Message
}

// Status returns HTTPResponse.Status
func (r PingResponse) Status() string {
	if r.HTTPResponse != nil {
		return r.HTTPResponse.Status
	}
	return http.StatusText(0)
}

// StatusCode returns HTTPResponse.StatusCode
func (r PingResponse) StatusCode() int {
	if r.HTTPResponse != nil {
		return r.HTTPResponse.StatusCode
	}
	return 0
}

// MetaDebugInstanceWithResponse request returning *MetaDebugInstanceResponse
func (c *Client) MetaDebugInstanceWithResponse(ctx context.Context, reqEditors ...RequestEditorFn) (*MetaDebugInstanceResponse, error) {
	req, err := newMetaDebugInstanceRequest(c.Server)
	if err != nil {
		return nil, err
	}
	req = req.WithContext(ctx)
	if err := c.applyEditors(ctx, req, reqEditors); err != nil {
		return nil, err
	}
	rsp, err := c.Client.Do(req)
	if err != nil {
		return nil, err
	}
	return parseMetaDebugInstanceResponse(rsp)
}

// MetaDebugSessionWithResponse request returning *MetaDebugSessionResponse
func (c *Client) MetaDebugSessionWithResponse(ctx context.Context, reqEditors ...RequestEditorFn) (*MetaDebugSessionResponse, error) {
	req, err := newMetaDebugSessionRequest(c.Server)
	if err != nil {
		return nil, err
	}
	req = req.WithContext(ctx)
	if err := c.applyEditors(ctx, req, reqEditors); err != nil {
		return nil, err
	}
	rsp, err := c.Client.Do(req)
	if err != nil {
		return nil, err
	}
	return parseMetaDebugSessionResponse(rsp)
}

// FileConversionByIDWithResponse request returning *FileConversionByIDResponse
func (c *Client) FileConversionByIDWithResponse(ctx context.Context, id string, reqEditors ...RequestEditorFn) (*FileConversionByIDResponse, error) {
	req, err := newFileConversionByIDRequest(c.Server, id)
	if err != nil {
		return nil, err
	}
	req = req.WithContext(ctx)
	if err := c.applyEditors(ctx, req, reqEditors); err != nil {
		return nil, err
	}
	rsp, err := c.Client.Do(req)
	if err != nil {
		return nil, err
	}
	return parseFileConversionByIDResponse(rsp)
}

// FileConvertWithBodyWithResponse request with arbitrary body returning *FileConvertResponse
func (c *Client) FileConvertWithBodyWithResponse(ctx context.Context, sourceFormat ValidFileTypes, outputFormat ValidFileTypes, contentType string, body io.Reader, reqEditors ...RequestEditorFn) (*FileConvertResponse, error) {
	req, err := newFileConvertRequestWithBody(c.Server, sourceFormat, outputFormat, contentType, body)
	if err != nil {
		return nil, err
	}
	req = req.WithContext(ctx)
	if err := c.applyEditors(ctx, req, reqEditors); err != nil {
		return nil, err
	}
	rsp, err := c.Client.Do(req)
	if err != nil {
		return nil, err
	}
	return parseFileConvertResponse(rsp)
}

// PingWithResponse request returning *PingResponse
func (c *Client) PingWithResponse(ctx context.Context, reqEditors ...RequestEditorFn) (*PingResponse, error) {
	req, err := newPingRequest(c.Server)
	if err != nil {
		return nil, err
	}
	req = req.WithContext(ctx)
	if err := c.applyEditors(ctx, req, reqEditors); err != nil {
		return nil, err
	}
	rsp, err := c.Client.Do(req)
	if err != nil {
		return nil, err
	}
	return parsePingResponse(rsp)
}

// parseMetaDebugInstanceResponse parses an HTTP response from a MetaDebugInstanceWithResponse call
func parseMetaDebugInstanceResponse(rsp *http.Response) (*MetaDebugInstanceResponse, error) {
	bodyBytes, err := ioutil.ReadAll(rsp.Body)
	defer func() { _ = rsp.Body.Close() }()
	if err != nil {
		return nil, err
	}

	response := &MetaDebugInstanceResponse{
		Body:         bodyBytes,
		HTTPResponse: rsp,
	}

	switch {
	case strings.Contains(rsp.Header.Get("Content-Type"), "json") && rsp.StatusCode == 200:
		var dest InstanceMetadata
		if err := json.Unmarshal(bodyBytes, &dest); err != nil {
			return nil, err
		}
		response.JSON200 = &dest

	case strings.Contains(rsp.Header.Get("Content-Type"), "json") && rsp.StatusCode == 400:
		var dest ErrorMessage
		if err := json.Unmarshal(bodyBytes, &dest); err != nil {
			return nil, err
		}
		response.JSON400 = &dest

	case strings.Contains(rsp.Header.Get("Content-Type"), "json") && rsp.StatusCode == 401:
		var dest ErrorMessage
		if err := json.Unmarshal(bodyBytes, &dest); err != nil {
			return nil, err
		}
		response.JSON401 = &dest

	case strings.Contains(rsp.Header.Get("Content-Type"), "json") && rsp.StatusCode == 403:
		var dest ErrorMessage
		if err := json.Unmarshal(bodyBytes, &dest); err != nil {
			return nil, err
		}
		response.JSON403 = &dest

	}

	return response, nil
}

// parseMetaDebugSessionResponse parses an HTTP response from a MetaDebugSessionWithResponse call
func parseMetaDebugSessionResponse(rsp *http.Response) (*MetaDebugSessionResponse, error) {
	bodyBytes, err := ioutil.ReadAll(rsp.Body)
	defer func() { _ = rsp.Body.Close() }()
	if err != nil {
		return nil, err
	}

	response := &MetaDebugSessionResponse{
		Body:         bodyBytes,
		HTTPResponse: rsp,
	}

	switch {
	case strings.Contains(rsp.Header.Get("Content-Type"), "json") && rsp.StatusCode == 200:
		var dest AuthSession
		if err := json.Unmarshal(bodyBytes, &dest); err != nil {
			return nil, err
		}
		response.JSON200 = &dest

	case strings.Contains(rsp.Header.Get("Content-Type"), "json") && rsp.StatusCode == 400:
		var dest ErrorMessage
		if err := json.Unmarshal(bodyBytes, &dest); err != nil {
			return nil, err
		}
		response.JSON400 = &dest

	case strings.Contains(rsp.Header.Get("Content-Type"), "json") && rsp.StatusCode == 401:
		var dest ErrorMessage
		if err := json.Unmarshal(bodyBytes, &dest); err != nil {
			return nil, err
		}
		response.JSON401 = &dest

	case strings.Contains(rsp.Header.Get("Content-Type"), "json") && rsp.StatusCode == 403:
		var dest ErrorMessage
		if err := json.Unmarshal(bodyBytes, &dest); err != nil {
			return nil, err
		}
		response.JSON403 = &dest

	}

	return response, nil
}

// parseFileConversionByIDResponse parses an HTTP response from a FileConversionByIDWithResponse call
func parseFileConversionByIDResponse(rsp *http.Response) (*FileConversionByIDResponse, error) {
	bodyBytes, err := ioutil.ReadAll(rsp.Body)
	defer func() { _ = rsp.Body.Close() }()
	if err != nil {
		return nil, err
	}

	response := &FileConversionByIDResponse{
		Body:         bodyBytes,
		HTTPResponse: rsp,
	}

	switch {
	case strings.Contains(rsp.Header.Get("Content-Type"), "json") && rsp.StatusCode == 200:
		var dest FileConversion
		if err := json.Unmarshal(bodyBytes, &dest); err != nil {
			return nil, err
		}
		response.JSON200 = &dest

	case strings.Contains(rsp.Header.Get("Content-Type"), "json") && rsp.StatusCode == 400:
		var dest ErrorMessage
		if err := json.Unmarshal(bodyBytes, &dest); err != nil {
			return nil, err
		}
		response.JSON400 = &dest

	case strings.Contains(rsp.Header.Get("Content-Type"), "json") && rsp.StatusCode == 401:
		var dest ErrorMessage
		if err := json.Unmarshal(bodyBytes, &dest); err != nil {
			return nil, err
		}
		response.JSON401 = &dest

	case strings.Contains(rsp.Header.Get("Content-Type"), "json") && rsp.StatusCode == 403:
		var dest ErrorMessage
		if err := json.Unmarshal(bodyBytes, &dest); err != nil {
			return nil, err
		}
		response.JSON403 = &dest

	case strings.Contains(rsp.Header.Get("Content-Type"), "json") && rsp.StatusCode == 404:
		var dest ErrorMessage
		if err := json.Unmarshal(bodyBytes, &dest); err != nil {
			return nil, err
		}
		response.JSON404 = &dest

	case strings.Contains(rsp.Header.Get("Content-Type"), "json") && rsp.StatusCode == 406:
		var dest ErrorMessage
		if err := json.Unmarshal(bodyBytes, &dest); err != nil {
			return nil, err
		}
		response.JSON406 = &dest

	}

	return response, nil
}

// parseFileConvertResponse parses an HTTP response from a FileConvertWithResponse call
func parseFileConvertResponse(rsp *http.Response) (*FileConvertResponse, error) {
	bodyBytes, err := ioutil.ReadAll(rsp.Body)
	defer func() { _ = rsp.Body.Close() }()
	if err != nil {
		return nil, err
	}

	response := &FileConvertResponse{
		Body:         bodyBytes,
		HTTPResponse: rsp,
	}

	switch {
	case strings.Contains(rsp.Header.Get("Content-Type"), "json") && rsp.StatusCode == 200:
		var dest FileConversion
		if err := json.Unmarshal(bodyBytes, &dest); err != nil {
			return nil, err
		}
		response.JSON200 = &dest

	case strings.Contains(rsp.Header.Get("Content-Type"), "json") && rsp.StatusCode == 202:
		var dest FileConversion
		if err := json.Unmarshal(bodyBytes, &dest); err != nil {
			return nil, err
		}
		response.JSON202 = &dest

	case strings.Contains(rsp.Header.Get("Content-Type"), "json") && rsp.StatusCode == 400:
		var dest ErrorMessage
		if err := json.Unmarshal(bodyBytes, &dest); err != nil {
			return nil, err
		}
		response.JSON400 = &dest

	case strings.Contains(rsp.Header.Get("Content-Type"), "json") && rsp.StatusCode == 401:
		var dest ErrorMessage
		if err := json.Unmarshal(bodyBytes, &dest); err != nil {
			return nil, err
		}
		response.JSON401 = &dest

	case strings.Contains(rsp.Header.Get("Content-Type"), "json") && rsp.StatusCode == 403:
		var dest ErrorMessage
		if err := json.Unmarshal(bodyBytes, &dest); err != nil {
			return nil, err
		}
		response.JSON403 = &dest

	case strings.Contains(rsp.Header.Get("Content-Type"), "json"):
		var dest ErrorMessage
		if err := json.Unmarshal(bodyBytes, &dest); err != nil {
			return nil, err
		}
		response.JSON406 = &dest

	}

	return response, nil
}

// parsePingResponse parses an HTTP response from a PingWithResponse call
func parsePingResponse(rsp *http.Response) (*PingResponse, error) {
	bodyBytes, err := ioutil.ReadAll(rsp.Body)
	defer func() { _ = rsp.Body.Close() }()
	if err != nil {
		return nil, err
	}

	response := &PingResponse{
		Body:         bodyBytes,
		HTTPResponse: rsp,
	}

	switch {
	case strings.Contains(rsp.Header.Get("Content-Type"), "json") && rsp.StatusCode == 200:
		var dest Message
		if err := json.Unmarshal(bodyBytes, &dest); err != nil {
			return nil, err
		}
		response.JSON200 = &dest

	}

	return response, nil
}

// MetaDebugInstance request returning *MetaDebugInstanceResponse
func (c *Client) MetaDebugInstance(ctx context.Context) (*InstanceMetadata, error) {
	req, err := newMetaDebugInstanceRequest(c.Server)
	if err != nil {
		return nil, err
	}
	req = req.WithContext(ctx)
	if err := c.applyEditors(ctx, req, c.RequestEditors); err != nil {
		return nil, err
	}
	ogrsp, err := c.Client.Do(req)
	if err != nil {
		return nil, err
	}
	rsp, err := parseMetaDebugInstanceResponse(ogrsp)
	if err != nil {
		return nil, fmt.Errorf("parsing response failed: %v", err)
	}
	// Check if the type we want to return is null.
	if rsp.JSON200 == nil {

		if rsp.JSON400 != nil {
			return nil, HTTPError{
				StatusCode: ogrsp.StatusCode,
				RequestURL: ogrsp.Request.URL,
				Message:    *rsp.JSON400.Message,
			}
		}

		if rsp.JSON401 != nil {
			return nil, HTTPError{
				StatusCode: ogrsp.StatusCode,
				RequestURL: ogrsp.Request.URL,
				Message:    *rsp.JSON401.Message,
			}
		}

		if rsp.JSON403 != nil {
			return nil, HTTPError{
				StatusCode: ogrsp.StatusCode,
				RequestURL: ogrsp.Request.URL,
				Message:    *rsp.JSON403.Message,
			}
		}

		b, err := ioutil.ReadAll(rsp.HTTPResponse.Body)
		if err != nil {
			return nil, fmt.Errorf("reading body failed: %w", err)
		}
		return nil, HTTPError{
			StatusCode: ogrsp.StatusCode,
			RequestURL: ogrsp.Request.URL,
			Message:    string(b),
		}
	}
	return rsp.JSON200, nil
}

// MetaDebugSession request returning *MetaDebugSessionResponse
func (c *Client) MetaDebugSession(ctx context.Context) (*AuthSession, error) {
	req, err := newMetaDebugSessionRequest(c.Server)
	if err != nil {
		return nil, err
	}
	req = req.WithContext(ctx)
	if err := c.applyEditors(ctx, req, c.RequestEditors); err != nil {
		return nil, err
	}
	ogrsp, err := c.Client.Do(req)
	if err != nil {
		return nil, err
	}
	rsp, err := parseMetaDebugSessionResponse(ogrsp)
	if err != nil {
		return nil, err
	}
	// Check if the type we want to return is null.
	if rsp.JSON200 == nil {

		if rsp.JSON400 != nil {
			return nil, HTTPError{
				StatusCode: ogrsp.StatusCode,
				RequestURL: ogrsp.Request.URL,
				Message:    *rsp.JSON400.Message,
			}
		}

		if rsp.JSON401 != nil {
			return nil, HTTPError{
				StatusCode: ogrsp.StatusCode,
				RequestURL: ogrsp.Request.URL,
				Message:    *rsp.JSON401.Message,
			}
		}

		if rsp.JSON403 != nil {
			return nil, HTTPError{
				StatusCode: ogrsp.StatusCode,
				RequestURL: ogrsp.Request.URL,
				Message:    *rsp.JSON403.Message,
			}
		}

		return nil, HTTPError{
			StatusCode: ogrsp.StatusCode,
			RequestURL: ogrsp.Request.URL,
			Message:    fmt.Sprintf("%#v", rsp),
		}
	}
	return rsp.JSON200, nil
}

// FileConversionByID request returning *FileConversionByIDResponse
func (c *Client) FileConversionByID(ctx context.Context, id string) (*FileConversion, error) {
	req, err := newFileConversionByIDRequest(c.Server, id)
	if err != nil {
		return nil, err
	}
	req = req.WithContext(ctx)
	if err := c.applyEditors(ctx, req, c.RequestEditors); err != nil {
		return nil, err
	}
	ogrsp, err := c.Client.Do(req)
	if err != nil {
		return nil, err
	}
	rsp, err := parseFileConversionByIDResponse(ogrsp)
	if err != nil {
		return nil, err
	}
	// Check if the type we want to return is null.
	if rsp.JSON200 == nil {

		if rsp.JSON400 != nil {
			return nil, HTTPError{
				StatusCode: ogrsp.StatusCode,
				RequestURL: ogrsp.Request.URL,
				Message:    *rsp.JSON400.Message,
			}
		}

		if rsp.JSON401 != nil {
			return nil, HTTPError{
				StatusCode: ogrsp.StatusCode,
				RequestURL: ogrsp.Request.URL,
				Message:    *rsp.JSON401.Message,
			}
		}

		if rsp.JSON403 != nil {
			return nil, HTTPError{
				StatusCode: ogrsp.StatusCode,
				RequestURL: ogrsp.Request.URL,
				Message:    *rsp.JSON403.Message,
			}
		}

		if rsp.JSON404 != nil {
			return nil, HTTPError{
				StatusCode: ogrsp.StatusCode,
				RequestURL: ogrsp.Request.URL,
				Message:    *rsp.JSON404.Message,
			}
		}

		if rsp.JSON406 != nil {
			return nil, HTTPError{
				StatusCode: ogrsp.StatusCode,
				RequestURL: ogrsp.Request.URL,
				Message:    *rsp.JSON406.Message,
			}
		}

		return nil, HTTPError{
			StatusCode: ogrsp.StatusCode,
			RequestURL: ogrsp.Request.URL,
			Message:    fmt.Sprintf("%#v", rsp),
		}
	}
	return rsp.JSON200, nil
}

// FileConvertWithBody request with arbitrary body returning *FileConvertResponse
func (c *Client) FileConvertWithBody(ctx context.Context, sourceFormat ValidFileTypes, outputFormat ValidFileTypes, contentType string, body io.Reader) (*FileConversion, error) {
	req, err := newFileConvertRequestWithBody(c.Server, sourceFormat, outputFormat, contentType, body)
	if err != nil {
		return nil, err
	}
	req = req.WithContext(ctx)
	if err := c.applyEditors(ctx, req, c.RequestEditors); err != nil {
		return nil, err
	}
	ogrsp, err := c.Client.Do(req)
	if err != nil {
		return nil, err
	}
	rsp, err := parseFileConvertResponse(ogrsp)
	if err != nil {
		return nil, err
	}
	// Check if the type we want to return is null.
	if rsp.JSON200 == nil {

		if rsp.JSON400 != nil {
			return nil, HTTPError{
				StatusCode: ogrsp.StatusCode,
				RequestURL: ogrsp.Request.URL,
				Message:    *rsp.JSON400.Message,
			}
		}

		if rsp.JSON401 != nil {
			return nil, HTTPError{
				StatusCode: ogrsp.StatusCode,
				RequestURL: ogrsp.Request.URL,
				Message:    *rsp.JSON401.Message,
			}
		}

		if rsp.JSON403 != nil {
			return nil, HTTPError{
				StatusCode: ogrsp.StatusCode,
				RequestURL: ogrsp.Request.URL,
				Message:    *rsp.JSON403.Message,
			}
		}

		if rsp.JSON406 != nil {
			return nil, HTTPError{
				StatusCode: ogrsp.StatusCode,
				RequestURL: ogrsp.Request.URL,
				Message:    *rsp.JSON406.Message,
			}
		}

		return nil, HTTPError{
			StatusCode: ogrsp.StatusCode,
			RequestURL: ogrsp.Request.URL,
			Message:    fmt.Sprintf("%#v", rsp),
		}
	}
	return rsp.JSON200, nil
}

// Ping request returning *PingResponse
func (c *Client) Ping(ctx context.Context) (*Message, error) {
	req, err := newPingRequest(c.Server)
	if err != nil {
		return nil, err
	}
	req = req.WithContext(ctx)
	if err := c.applyEditors(ctx, req, c.RequestEditors); err != nil {
		return nil, err
	}
	ogrsp, err := c.Client.Do(req)
	if err != nil {
		return nil, err
	}
	rsp, err := parsePingResponse(ogrsp)
	if err != nil {
		return nil, err
	}
	// Check if the type we want to return is null.
	if rsp.JSON200 == nil {

		return nil, HTTPError{
			StatusCode: ogrsp.StatusCode,
			RequestURL: ogrsp.Request.URL,
			Message:    fmt.Sprintf("%#v", rsp),
		}
	}
	return rsp.JSON200, nil
}
