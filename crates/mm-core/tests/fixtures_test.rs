/// Integration test demonstrating use of test fixtures
///
/// This test verifies that all test fixtures are accessible and parseable.
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures")
}

#[test]
fn test_observing_scenario_fixtures_exist() {
    let observing_dir = fixtures_dir().join("observing_scenarios");
    assert!(
        observing_dir.exists(),
        "Observing scenarios fixtures directory should exist"
    );

    // Check O4HL .dat files
    let coincs = observing_dir.join("coincs.dat");
    assert!(coincs.exists(), "O4HL coincs.dat should exist");
    assert!(
        coincs.metadata().unwrap().len() > 0,
        "O4HL coincs.dat should not be empty"
    );

    let allsky = observing_dir.join("allsky.dat");
    assert!(allsky.exists(), "O4HL allsky.dat should exist");

    let injections = observing_dir.join("injections.dat");
    assert!(injections.exists(), "O4HL injections.dat should exist");

    // Check FITS skymap files (0.fits through 9.fits)
    for i in 0..10 {
        let skymap = observing_dir.join(format!("{}.fits", i));
        assert!(skymap.exists(), "{}.fits should exist", i);
        assert!(
            skymap.metadata().unwrap().len() > 0,
            "{}.fits should not be empty",
            i
        );
    }

    println!(
        "✓ Found {} observing scenario files (3 .dat + 10 .fits)",
        std::fs::read_dir(&observing_dir).unwrap().count()
    );
}

#[test]
fn test_grb_xml_fixtures_exist() {
    let grb_dir = fixtures_dir().join("grb_xmls");
    assert!(grb_dir.exists(), "GRB XMLs fixtures directory should exist");

    // Check for expected GRB XML files
    let expected_files = vec![
        "fermi_grb_gcn.xml",
        "swift_grb_gcn.xml",
        "fermi_initial_grb_gcn.xml",
        "fermi_subthresh_grb_gcn.xml",
        "einsteinprobe_grb_template.xml",
        "svom_grb_gcn.xml",
        "svom_grb_template.xml",
        "fermi_subgrbtargeted_template.xml",
        "GRB180116A_Fermi_GBM_Gnd_Pos.xml",
        "fermi_subthresh_grb_lowconfidence.xml",
    ];

    for filename in &expected_files {
        let path = grb_dir.join(filename);
        assert!(path.exists(), "{} should exist", filename);

        // Verify it's a valid XML file by checking it starts with <?xml or <voe:VOEvent
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(
            content.starts_with("<?xml") || content.starts_with("<voe:VOEvent"),
            "{} should be valid XML",
            filename
        );
    }

    println!("✓ Found {} GRB XML files", expected_files.len());
}

#[test]
fn test_optical_lightcurve_fixtures_exist() {
    let lc_dir = fixtures_dir().join("lightcurves_csv");
    assert!(
        lc_dir.exists(),
        "Light curves fixtures directory should exist"
    );

    // Check for expected ZTF CSV files
    let expected_files = vec![
        "ZTF25aaaalin.csv",
        "ZTF25aaaawig.csv",
        "ZTF25aaabezb.csv",
        "ZTF25aaabnwi.csv",
        "ZTF25aaabnxh.csv",
        "ZTF25aaacrjj.csv",
        "ZTF25aaadqsi.csv",
        "ZTF25aaadytl.csv",
        "ZTF25aaaecsu.csv",
        "ZTF25aaaeykb.csv",
    ];

    for filename in &expected_files {
        let path = lc_dir.join(filename);
        assert!(path.exists(), "{} should exist", filename);

        // Verify it's a valid CSV by checking the header
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(
            content.starts_with("mjd,flux,flux_err,filter")
                || content.starts_with("jd,flux,flux_err,band")
                || content.starts_with("objectId,jd,flux,flux_err,band"),
            "{} should have CSV header, got: {}",
            filename,
            content.lines().next().unwrap_or("")
        );
    }

    println!("✓ Found {} optical light curve files", expected_files.len());
}

#[test]
fn test_parse_observing_scenario_data() {
    let coincs_file = fixtures_dir()
        .join("observing_scenarios")
        .join("coincs.dat");

    let content = std::fs::read_to_string(&coincs_file).unwrap();
    let lines: Vec<&str> = content.lines().collect();

    assert!(lines.len() > 10, "Should have multiple coinc events");

    // Parse first non-comment, non-header line
    let first_data_line = lines
        .iter()
        .find(|l| !l.starts_with('#') && !l.starts_with("coinc_event_id"))
        .unwrap();
    let fields: Vec<&str> = first_data_line.split('\t').collect();

    assert!(
        fields.len() >= 3,
        "Coinc data should have at least 3 fields (id, ifos, snr), got: {}",
        fields.len()
    );

    // First field should be parseable as a number (event ID)
    let _event_id: u32 = fields[0].parse().expect("First field should be event ID");

    // Third field should be parseable as SNR
    let _snr: f64 = fields[2].parse().expect("Third field should be SNR");

    println!("✓ Parsed observing scenario with {} lines", lines.len());
}

#[test]
fn test_parse_optical_lightcurve() {
    let lc_file = fixtures_dir()
        .join("lightcurves_csv")
        .join("ZTF25aaabnwi.csv"); // This one has the most data points

    let content = std::fs::read_to_string(&lc_file).unwrap();
    let lines: Vec<&str> = content.lines().collect();

    assert!(lines.len() > 1, "Should have header + data");

    // Skip header and parse first data line
    if lines.len() > 1 {
        let data_line = lines[1];
        let fields: Vec<&str> = data_line.split(',').collect();

        // Expect: mjd, flux, flux_err, filter
        assert!(
            fields.len() >= 4,
            "CSV should have at least 4 fields: {}",
            data_line
        );

        // Parse MJD
        let _mjd: f64 = fields[0].parse().expect("First field should be MJD");

        // Parse flux
        let _flux: f64 = fields[1].parse().expect("Second field should be flux");

        println!("✓ Parsed light curve with {} measurements", lines.len() - 1);
    }
}

#[test]
fn test_all_fixtures_total_size() {
    let fixtures = fixtures_dir();

    let mut total_size = 0u64;
    let mut file_count = 0usize;

    // Walk through all fixture files
    for entry in walkdir::WalkDir::new(&fixtures)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            total_size += entry.metadata().unwrap().len();
            file_count += 1;
        }
    }

    println!(
        "✓ Test fixtures: {} files, {:.2} MB total",
        file_count,
        total_size as f64 / 1_048_576.0
    );

    assert!(
        file_count >= 33,
        "Should have at least 33 fixture files: 10 GRB XMLs + 10 ZTF CSVs + 3 O4HL .dat + 10 O4HL .fits (got {})",
        file_count
    );
    assert!(total_size > 8_000_000, "Should have at least 8MB of data (10 FITS files @ 759K each + other files)");
}
