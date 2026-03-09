# FFix | FileFix

A terminal utility with interactive wizard for converting images, documents, and Jupyter notebooks to better suited file formats. Primarily built to avoid dealing with heic files airdropped from iPhone. As no one accepts heic files, and converting then manually is a pain. Also supports docx, pptx, and ipynb conversions to pdf. Built in Rust. Heavily vibecoded.

---

## Supported Formats

| Input File Type | Supported Extensions                                | Available Output Formats                     |
| :-------------- | :-------------------------------------------------- | :------------------------------------------- |
| **Images**      | `heic`, `heif`, `tiff`, `bmp`, `jpg`, `jpeg`, `png` | Any other image format in the list, or `pdf` |
| **Documents**   | `docx`, `pptx`                                      | `pdf`                                        |
| **Notebooks**   | `ipynb`                                             | `pdf`                                        |

## Prerequisites / External Dependencies

- **Image Conversions:** Requires **ImageMagick** (`magick`).
- **Document Conversions:** Requires **LibreOffice** (`soffice`). _(On macOS, FileFix automatically looks in `/Applications/LibreOffice.app/...`)_
- **Jupyter Notebooks:** Requires **Jupyter** (`jupyter nbconvert`).

FileFix will automatically check for these dependencies and warn you if they are missing before attempting a conversion.

## Installation

Currently, you can build and install FileFix from source using Cargo:

```bash
git clone https://github.com/perhenrikgithub/FileFix.git
cd filefix
cargo install --path .
```

## Usage

### 0. Set Up Default Folder (Optional)

By default, FileFix looks for files in your system's `Downloads` folder. You can change this default folder by running:

```bash
filefix config --default-folder /path/to/your/custom/folder
```

### 1. Use the Interactive Wizard

Simply run `filefix <convert_from_filetype>` in your terminal, and follow the prompts to select the file type, choose files from the default folder (or select all), and specify output formats.

```bash
filefix heic
filefix docx
```

### 2. Verbose CLI Commands

For scripting or power users, you can bypass the interactive prompts entirely using the CLI commands (the wizard is built on top of these commands):

**Batch Conversion:**

```bash
filefix convert-batch --input-type heic --to jpg
```

**Single File Conversion:**

```bash
filefix convert-single --file "my_document.docx" --to pdf
```

### CLI Flags & Advanced Options

When using `convert-single` or `convert-batch`, you can append the following flags:

- `--delete-original`: Deletes the source file after a successful conversion.
- `--overwrite`: Forces overwriting if the target filename already exists. (If omitted, FileFix safely appends `(1)`, `(2)`, etc. to the filename).
- `--open-when-done`: Automatically opens the converted files using your OS's default viewer (`open` on Mac, `xdg-open` on Linux, `start` on Windows).

## Configuration

FileFix saves its configuration locally at:

- `~/.filefix/config.toml`

You can manually edit this file or use the `filefix config` command to update your preferences (recomended).
