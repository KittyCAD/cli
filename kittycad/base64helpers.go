package kittycad

import (
	"bytes"
	"encoding/base64"
	"fmt"
)

// FileConversionByIDWithBase64Helper returns the status of a file conversion.
// This function will automatically base64 decode the contents of the result output.
//
// This function is a wrapper around the FileConversionByID function.
func (c *Client) FileConversionByIDWithBase64Helper(id string) (*FileConversion, []byte, error) {
	resp, err := c.FileConversionByID(id)
	if err != nil {
		return nil, nil, err
	}

	if resp.Output == "" {
		return resp, nil, nil
	}

	// Decode the base64 encoded body.
	output, err := base64.StdEncoding.DecodeString(resp.Output)
	if err != nil {
		return nil, nil, fmt.Errorf("base64 decoding output from API failed: %v", err)
	}

	return resp, output, nil
}

// FileConvertWithBase64Helper converts a file.
// This function will automatically base64 encode and decode the contents of the
// src file and output file.
//
// This function is a wrapper around the FileConvert function.
func (c *Client) FileConvertWithBase64Helper(srcFormat ValidFileType, outputFormat ValidFileType, body []byte) (*FileConversion, []byte, error) {
	var b bytes.Buffer
	encoder := base64.NewEncoder(base64.StdEncoding, &b)
	// Encode the body as base64.
	encoder.Write(body)
	// Must close the encoder when finished to flush any partial blocks.
	// If you comment out the following line, the last partial block "r"
	// won't be encoded.
	encoder.Close()
	resp, err := c.FileConvert(srcFormat, outputFormat, &b)
	if err != nil {
		return nil, nil, err
	}

	if resp.Output == "" {
		return resp, nil, nil
	}

	// Decode the base64 encoded body.
	output, err := base64.StdEncoding.DecodeString(resp.Output)
	if err != nil {
		return nil, nil, fmt.Errorf("base64 decoding output from API failed: %v", err)
	}

	return resp, output, nil
}
