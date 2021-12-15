package shared

import (
	"fmt"
	"time"

	"github.com/cli/cli/v2/pkg/iostreams"
	"github.com/docker/go-units"
	"github.com/kittycad/cli/kittycad"
)

// FormattedStatus formats a file conversion status with color.
func FormattedStatus(cs *iostreams.ColorScheme, status kittycad.FileConversionStatus) string {
	var colorFunc func(string) string
	switch status {
	case kittycad.FileConversionStatusCompleted:
		colorFunc = cs.Green
	case kittycad.FileConversionStatusFailed:
		colorFunc = cs.Red
	case kittycad.FileConversionStatusInProgress:
		colorFunc = cs.Yellow
	case kittycad.FileConversionStatusQueued:
		colorFunc = cs.Cyan
	case kittycad.FileConversionStatusUploaded:
		colorFunc = cs.Blue
	default:
		colorFunc = func(str string) string { return str } // Do nothing
	}

	return colorFunc(string(status))
}

// PrintRawConversion prints the raw output of a conversion.
func PrintRawConversion(io *iostreams.IOStreams, conversion *kittycad.FileConversion, output []byte, outputFile string, duration time.Duration) error {
	out := io.Out
	cs := io.ColorScheme()

	fmt.Fprintf(out, "id:\t\t%s\n", conversion.ID)
	fmt.Fprintf(out, "status:\t\t%s\n", FormattedStatus(cs, conversion.Status))
	fmt.Fprintf(out, "created at:\t%s\n", conversion.CreatedAt)
	if conversion.CompletedAt != nil && !conversion.CompletedAt.IsZero() && conversion.CompletedAt.Unix() != 0 {
		fmt.Fprintf(out, "completed at:\t%s\n", conversion.CompletedAt)
	}
	fmt.Fprintf(out, "duration:\t\t%s\n", units.HumanDuration(duration))
	fmt.Fprintf(out, "source format:\t%s\n", conversion.SrcFormat)
	fmt.Fprintf(out, "output format:\t%s\n", conversion.OutputFormat)
	if outputFile != "" && len(output) > 0 {
		fmt.Fprintf(out, "output file:\t%s\n", outputFile)
	}

	// Write the output to stdout.
	if len(output) > 0 && outputFile == "" {
		out.Write(output)
	}

	return nil
}

// PrintHumanConversion prints the human-readable output of a conversion.
func PrintHumanConversion(io *iostreams.IOStreams, conversion *kittycad.FileConversion, output []byte, outputFile string, duration time.Duration) error {
	out := io.Out
	cs := io.ColorScheme()

	// Source -> Output
	fmt.Fprintf(out, "%s -> %s\t%s\n", string(conversion.SrcFormat), cs.Bold(string(conversion.OutputFormat)), FormattedStatus(cs, conversion.Status))

	// Print that we have saved the output to a file.
	if outputFile != "" && len(output) > 0 {
		fmt.Fprintf(out, "Output has been saved to %s\n", cs.Bold(outputFile))
	}

	// Print the time.
	if conversion.CompletedAt != nil && !conversion.CompletedAt.IsZero() && conversion.CompletedAt.Unix() != 0 {
		fmt.Fprintf(out, "\nConversion took %s\n\n", units.HumanDuration(duration))
	} else {
		if conversion.Status != kittycad.FileConversionStatusUploaded {
			fmt.Fprintf(out, "\nConversion `%s` has been running for %s\n\n", conversion.ID, units.HumanDuration(duration))
		} else {
			fmt.Fprintf(out, "\nGet the status of your conversion with `kittycad file status %s`\n\n", conversion.ID)
		}
	}

	// Write the output to stdout.
	if len(output) > 0 && outputFile == "" {
		out.Write(output)
	}

	return nil
}
