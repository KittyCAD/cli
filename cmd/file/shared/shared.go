package shared

import (
	"fmt"
	"time"

	"github.com/cli/cli/v2/pkg/iostreams"
	"github.com/docker/go-units"
	"github.com/kittycad/kittycad.go"
)

// FormattedStatus formats a file conversion status with color.
func FormattedStatus(cs *iostreams.ColorScheme, status kittycad.APICallStatus) string {
	var colorFunc func(string) string
	switch status {
	case kittycad.APICallStatusCompleted:
		colorFunc = cs.Green
	case kittycad.APICallStatusFailed:
		colorFunc = cs.Red
	case kittycad.APICallStatusInProgress:
		colorFunc = cs.Yellow
	case kittycad.APICallStatusQueued:
		colorFunc = cs.Cyan
	case kittycad.APICallStatusUploaded:
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

// PrintRawAsyncAPICall prints the raw output of an async API call.
func PrintRawAsyncAPICall(io *iostreams.IOStreams, asyncAPICall *kittycad.AsyncAPICallOutput, output []byte, outputFile string, duration time.Duration) error {
	out := io.Out
	cs := io.ColorScheme()

	fmt.Fprintf(out, "id:\t\t%s\n", asyncAPICall.ID)
	fmt.Fprintf(out, "status:\t\t%s\n", FormattedStatus(cs, asyncAPICall.Status))
	fmt.Fprintf(out, "created at:\t%s\n", asyncAPICall.CreatedAt)
	if asyncAPICall.CompletedAt != nil && !asyncAPICall.CompletedAt.IsZero() && asyncAPICall.CompletedAt.Unix() != 0 {
		fmt.Fprintf(out, "completed at:\t%s\n", asyncAPICall.CompletedAt)
	}
	fmt.Fprintf(out, "duration:\t\t%s\n", units.HumanDuration(duration))
	fmt.Fprintf(out, "source format:\t%s\n", asyncAPICall.SrcFormat)

	if asyncAPICall.Type == string(kittycad.AsyncAPICallOutputTypeFileConversion) {
		fmt.Fprintf(out, "output format:\t%s\n", asyncAPICall.OutputFormat)

		if outputFile != "" && len(output) > 0 {
			fmt.Fprintf(out, "output file:\t%s\n", outputFile)
		}

		// Write the output to stdout.
		if len(output) > 0 && outputFile == "" {
			out.Write(output)
		}
	}

	if asyncAPICall.Mass > 0.0 {
		fmt.Fprintf(out, "\nmaterial density: %f\n\n", asyncAPICall.MaterialDensity)
		fmt.Fprintf(out, "\nmass: %f\n\n", asyncAPICall.Mass)
	}
	if asyncAPICall.Volume > 0.0 {
		fmt.Fprintf(out, "\nvolume: %f\n\n", asyncAPICall.Volume)
	}
	if asyncAPICall.Density > 0.0 {
		fmt.Fprintf(out, "\nmaterial mass: %f\n\n", asyncAPICall.MaterialMass)
		fmt.Fprintf(out, "\ndensity: %f\n\n", asyncAPICall.Density)
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
	if conversion.CompletedAt != nil && conversion.CompletedAt.Time != nil && !conversion.CompletedAt.IsZero() && conversion.CompletedAt.Unix() != 0 {
		fmt.Fprintf(out, "\nConversion took %s\n\n", units.HumanDuration(duration))
	} else {
		if conversion.Status != kittycad.APICallStatusUploaded {
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

// PrintHumanAsyncAPICallOutput prints the human-readable output of an async API call.
func PrintHumanAsyncAPICallOutput(io *iostreams.IOStreams, asyncAPICall *kittycad.AsyncAPICallOutput, output []byte, outputFile string, duration time.Duration) error {
	out := io.Out
	cs := io.ColorScheme()

	if asyncAPICall.Type == string(kittycad.AsyncAPICallOutputTypeFileConversion) {
		// Source -> Output
		fmt.Fprintf(out, "%s -> %s\t%s\n", string(asyncAPICall.SrcFormat), cs.Bold(string(asyncAPICall.OutputFormat)), FormattedStatus(cs, asyncAPICall.Status))

		// Print that we have saved the output to a file.
		if outputFile != "" && len(output) > 0 {
			fmt.Fprintf(out, "Output has been saved to %s\n", cs.Bold(outputFile))
		}
	}

	// Print the time.
	if asyncAPICall.CompletedAt != nil && asyncAPICall.CompletedAt.Time != nil && !asyncAPICall.CompletedAt.IsZero() && asyncAPICall.CompletedAt.Unix() != 0 {
		fmt.Fprintf(out, "\nAPI call took %s\n\n", units.HumanDuration(duration))
	} else {
		if asyncAPICall.Status != kittycad.APICallStatusUploaded {
			fmt.Fprintf(out, "\nAPI call `%s` has been running for %s\n\n", asyncAPICall.ID, units.HumanDuration(duration))
		} else {
			fmt.Fprintf(out, "\nGet the status of the API call with `kittycad api-call status %s`\n\n", asyncAPICall.ID)
		}
	}

	// Write the output to stdout.
	if len(output) > 0 && outputFile == "" {
		out.Write(output)
	}

	if asyncAPICall.Mass > 0.0 {
		fmt.Fprintf(out, "\n%s: %f\n\n", cs.Bold(string("material density:")), asyncAPICall.MaterialDensity)
		fmt.Fprintf(out, "\n%s: %f\n\n", cs.Bold(string("mass:")), asyncAPICall.Mass)
	}
	if asyncAPICall.Volume > 0.0 {
		fmt.Fprintf(out, "\n%s: %f\n\n", cs.Bold(string("volume:")), asyncAPICall.Volume)
	}
	if asyncAPICall.Density > 0.0 {
		fmt.Fprintf(out, "\n%s: %f\n\n", cs.Bold(string("material mass:")), asyncAPICall.MaterialMass)
		fmt.Fprintf(out, "\n%s: %f\n\n", cs.Bold(string("density:")), asyncAPICall.Density)
	}

	return nil
}
