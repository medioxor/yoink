use chrono::{NaiveDateTime, TimeZone, Utc};
use nt_time::FileTime;
use ntfs::Ntfs;
use ntfs::{
    attribute_value::NtfsAttributeValue,
    indexes::NtfsFileNameIndex,
    structured_values::{NtfsAttributeList, NtfsStandardInformation},
    NtfsAttribute, NtfsAttributeFlags, NtfsAttributeType, NtfsFile, NtfsReadSeek,
};
use std::{
    error::Error,
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
};
use std::{fs::File, io::BufReader};

pub fn parse_stream(path: &str) -> (String, String) {
    if let Some(pos) = path.rfind(':') {
        if pos == 1 {
            (path.to_string(), String::new())
        } else {
            let (file_path, stream_name) = path.split_at(pos);
            let stream_name = stream_name.replace(":", "");
            (file_path.to_string(), stream_name)
        }
    } else {
        (path.to_string(), String::new())
    }
}

pub fn copy_file<W>(file_path: String, mut writer: W) -> Result<usize, Box<dyn Error>>
where
    W: Write,
{
    let (path, stream_name) = parse_stream(file_path.as_str());
    let drive_letter = path.chars().next().ok_or("Invalid path")?;
    let mut drive = open_drive(drive_letter.to_string())?;
    let file = open_file(path, &mut drive.filesystem_reader, &drive.ntfs)?;

    let data_item = file
        .data(&mut drive.filesystem_reader, stream_name.as_str())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("The file does not have a stream called {}", stream_name),
            )
        })??;

    let mut starting_position: u64 = 0;

    if data_item
        .to_attribute()?
        .flags()
        .contains(NtfsAttributeFlags::SPARSE)
    {
        let attributes = file.attributes_raw();
        for attribute in attributes {
            let attribute = attribute?;
            let ty = attribute.ty()?;
            if ty == NtfsAttributeType::AttributeList {
                let list = attribute
                    .structured_value::<_, NtfsAttributeList>(&mut drive.filesystem_reader)?;
                let mut list_iter = list.entries();
                while let Some(entry) = list_iter.next(&mut drive.filesystem_reader) {
                    let entry = entry?;
                    let entry_record_number = entry.base_file_reference().file_record_number();
                    if entry_record_number == file.file_record_number() {
                        continue;
                    }
                    let entry_file = entry.to_file(&drive.ntfs, &mut drive.filesystem_reader)?;
                    let entry_attribute: NtfsAttribute<'_, '_> = entry.to_attribute(&entry_file)?;
                    let value = entry_attribute.value(&mut drive.filesystem_reader)?;
                    if let NtfsAttributeValue::NonResident(non_resident_value) = value {
                        for data_run in non_resident_value.data_runs() {
                            let data_run = data_run?;
                            if data_run.data_position() == None.into() {
                                starting_position = data_run.allocated_size();
                            }
                        }
                    }
                }
            }
        }
    }

    let data_attribute = data_item.to_attribute()?;
    let mut data_value = data_attribute.value(&mut drive.filesystem_reader)?;
    let mut buf = [0u8; 4096];
    data_value.seek(
        &mut drive.filesystem_reader,
        std::io::SeekFrom::Start(starting_position),
    )?;

    loop {
        let bytes_read = data_value.read(&mut drive.filesystem_reader, &mut buf)?;
        if bytes_read == 0 {
            break;
        }
        match writer.write_all(buf.as_ref()) {
            Ok(_) => continue,
            Err(e) => {
                println!("Finished writing to zip file: {}", e);
                break;
            }
        }
    }

    Ok(100)
}

struct Drive {
    filesystem_reader: BufReader<SectorReader<File>>,
    ntfs: Ntfs,
}

fn open_drive(drive_letter: String) -> Result<Drive, Box<dyn Error>> {
    let volume_path = format!("\\\\.\\{}:", drive_letter);
    let volume = File::open(Path::new(&volume_path))?;
    let sector_reader = SectorReader::new(volume, 4096)?;
    let mut filesystem_reader = BufReader::new(sector_reader);
    let mut ntfs = Ntfs::new(&mut filesystem_reader)?;
    ntfs.read_upcase_table(&mut filesystem_reader)?;

    Ok(Drive {
        filesystem_reader,
        ntfs,
    })
}

fn open_file<'f>(
    file_path: String,
    filesystem_reader: &mut BufReader<SectorReader<File>>,
    ntfs: &'f Ntfs,
) -> Result<NtfsFile<'f>, Box<dyn Error>> {
    let mut current_directory: Vec<NtfsFile> = vec![ntfs.root_directory(filesystem_reader)?];

    for dir in Path::new(&file_path).iter() {
        let next_dir = dir.to_str().ok_or("Invalid path")?;
        let index = current_directory
            .last()
            .unwrap()
            .directory_index(filesystem_reader)?;
        let mut finder = index.finder();

        if let Some(entry) = NtfsFileNameIndex::find(&mut finder, ntfs, filesystem_reader, next_dir)
        {
            let file = entry?.to_file(ntfs, filesystem_reader)?;
            if !file.is_directory() {
                return Ok(file);
            } else {
                current_directory.push(file);
            }
        }
    }

    Err("File not found".into())
}

pub fn does_file_exist(drive_letter: String, file_path: String) -> Result<bool, Box<dyn Error>> {
    let mut drive = open_drive(drive_letter.to_string())?;
    match open_file(file_path, &mut drive.filesystem_reader, &drive.ntfs) {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}

pub fn get_lastmodified(file_path: String) -> Result<NaiveDateTime, Box<dyn Error>> {
    let drive_letter = file_path.chars().next().ok_or("Invalid path")?;
    let mut drive = open_drive(drive_letter.to_string())?;
    let file = open_file(file_path, &mut drive.filesystem_reader, &drive.ntfs)?;

    let mut attributes = file.attributes();

    while let Some(attribute_item) = attributes.next(&mut drive.filesystem_reader) {
        let attribute_item = attribute_item?;
        let attribute = attribute_item.to_attribute()?;

        if let Ok(NtfsAttributeType::StandardInformation) = attribute.ty() {
            let std_info = attribute.resident_structured_value::<NtfsStandardInformation>()?;
            let file_time = FileTime::from(std_info.mft_record_modification_time().nt_timestamp())
                .to_unix_time_secs();
            let modified_timestamp = Utc
                .timestamp_opt(file_time, 0)
                .single()
                .ok_or("Invalid timestamp")?;
            return Ok(modified_timestamp.naive_utc());
        }
    }

    Err("No standard information attribute found".into())
}

/// `SectorReader` encapsulates any reader and only performs read and seek operations on it
/// on boundaries of the given sector size.
///
/// This can be very useful for readers that only accept sector-sized reads (like reading
/// from a raw partition on Windows).
/// The sector size must be a power of two.
///
/// This reader does not keep any buffer.
/// You are advised to encapsulate `SectorReader` in a buffered reader, as unbuffered reads of
/// just a few bytes here and there are highly inefficient.
pub struct SectorReader<R>
where
    R: Read + Seek,
{
    /// The inner reader stream.
    inner: R,
    /// The sector size set at creation.
    sector_size: usize,
    /// The current stream position as requested by the caller through `read` or `seek`.
    /// The implementation will internally make sure to only read/seek on sector boundaries.
    stream_position: u64,
    /// This buffer is only part of the struct as a small performance optimization (keeping it allocated between reads).
    temp_buf: Vec<u8>,
}

impl<R> SectorReader<R>
where
    R: Read + Seek,
{
    pub fn new(inner: R, sector_size: usize) -> io::Result<Self> {
        if !sector_size.is_power_of_two() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "sector_size is not a power of two",
            ));
        }

        Ok(Self {
            inner,
            sector_size,
            stream_position: 0,
            temp_buf: Vec::new(),
        })
    }

    fn align_down_to_sector_size(&self, n: u64) -> u64 {
        n / self.sector_size as u64 * self.sector_size as u64
    }

    fn align_up_to_sector_size(&self, n: u64) -> u64 {
        self.align_down_to_sector_size(n) + self.sector_size as u64
    }
}

impl<R> Read for SectorReader<R>
where
    R: Read + Seek,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // We can only read from a sector boundary, and `self.stream_position` specifies the position where the
        // caller thinks we are.
        // Align down to a sector boundary to determine the position where we really are (see our `seek` implementation).
        let aligned_position = self.align_down_to_sector_size(self.stream_position);

        // We have to read more bytes now to make up for the alignment difference.
        // We can also only read in multiples of the sector size, so align up to the next sector boundary.
        let start = (self.stream_position - aligned_position) as usize;
        let end = start + buf.len();
        let aligned_bytes_to_read = self.align_up_to_sector_size(end as u64) as usize;

        // Perform the sector-sized read and copy the actually requested bytes into the given buffer.
        self.temp_buf.resize(aligned_bytes_to_read, 0);
        self.inner.read_exact(&mut self.temp_buf)?;
        buf.copy_from_slice(&self.temp_buf[start..end]);

        // We are done.
        self.stream_position += buf.len() as u64;
        Ok(buf.len())
    }
}

impl<R> Seek for SectorReader<R>
where
    R: Read + Seek,
{
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let new_pos = match pos {
            SeekFrom::Start(n) => Some(n),
            SeekFrom::End(_n) => {
                // This is unsupported, because it's not safely possible under Windows.
                // We cannot seek to the end to determine the raw partition size.
                // Which makes it impossible to set `self.stream_position`.
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "SeekFrom::End is unsupported for SectorReader",
                ));
            }
            SeekFrom::Current(n) => {
                if n >= 0 {
                    self.stream_position.checked_add(n as u64)
                } else {
                    self.stream_position.checked_sub(n.wrapping_neg() as u64)
                }
            }
        };

        match new_pos {
            Some(n) => {
                // We can only seek on sector boundaries, so align down the requested seek position and seek to that.
                let aligned_n = self.align_down_to_sector_size(n);
                self.inner.seek(SeekFrom::Start(aligned_n))?;

                // Make the caller believe that we seeked to the actually requested position.
                // Our `read` implementation will cover the difference.
                self.stream_position = n;
                Ok(self.stream_position)
            }
            None => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid seek to a negative or overflowing position",
            )),
        }
    }
}
