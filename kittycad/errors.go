package kittycad

import (
	"encoding/json"
	"fmt"
	"io/ioutil"
	"net/http"
	"net/url"
)

// HTTPError is an error returned by a failed API call.
type HTTPError struct {
	// URL is the URL that was being accessed when the error occurred.
	// It will always be populated.
	URL *url.URL
	// StatusCode is the HTTP response status code and will always be populated.
	StatusCode int
	// Message is the server response message and is only populated when
	// explicitly referenced by the JSON server response.
	Message string
	// Body is the raw response returned by the server.
	// It is often but not always JSON, depending on how the request fails.
	Body string
	// Header contains the response header fields from the server.
	Header http.Header
}

// Error converts the Error type to a readable string.
func (err HTTPError) Error() string {
	if err.Message != "" {
		return fmt.Sprintf("HTTP %d: %s (%s)", err.StatusCode, err.Message, err.URL)
	}

	return fmt.Sprintf("HTTP %d (%s) BODY -> %v", err.StatusCode, err.URL, err.Body)
}

// checkResponse returns an error (of type *HTTPError) if the response
// status code is not 2xx.
func checkResponse(res *http.Response) error {
	if res.StatusCode >= 200 && res.StatusCode <= 299 {
		return nil
	}

	slurp, err := ioutil.ReadAll(res.Body)
	if err == nil {
		var jerr ErrorMessage

		// Try to decode the body as an ErrorMessage.
		if err := json.Unmarshal(slurp, &jerr); err == nil {
			return &HTTPError{
				URL:        res.Request.URL,
				StatusCode: res.StatusCode,
				Message:    jerr.Message,
				Body:       string(slurp),
				Header:     res.Header,
			}
		}
	}

	return &HTTPError{
		URL:        res.Request.URL,
		StatusCode: res.StatusCode,
		Body:       string(slurp),
		Header:     res.Header,
		Message:    "",
	}
}
