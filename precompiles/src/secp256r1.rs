use {
    agave_feature_set::FeatureSet,
    solana_precompile_error::PrecompileError,
    solana_secp256r1_program::{
        Secp256r1SignatureOffsets, COMPRESSED_PUBKEY_SERIALIZED_SIZE, FIELD_SIZE,
        SECP256R1_HALF_ORDER, SECP256R1_ORDER_MINUS_ONE, SIGNATURE_OFFSETS_SERIALIZED_SIZE,
        SIGNATURE_OFFSETS_START, SIGNATURE_SERIALIZED_SIZE,
    },
};

pub fn verify(
    data: &[u8],
    instruction_datas: &[&[u8]],
    _feature_set: &FeatureSet,
) -> Result<(), PrecompileError> {
    Err(PrecompileError::InvalidInstructionDataSize)    // Patching this function out, as it uses OpenSSL!
}

fn get_data_slice<'a>(
    data: &'a [u8],
    instruction_datas: &'a [&[u8]],
    instruction_index: u16,
    offset_start: u16,
    size: usize,
) -> Result<&'a [u8], PrecompileError> {
    let instruction = if instruction_index == u16::MAX {
        data
    } else {
        let signature_index = instruction_index as usize;
        if signature_index >= instruction_datas.len() {
            return Err(PrecompileError::InvalidDataOffsets);
        }
        instruction_datas[signature_index]
    };

    let start = offset_start as usize;
    let end = start.saturating_add(size);
    if end > instruction.len() {
        return Err(PrecompileError::InvalidDataOffsets);
    }

    Ok(&instruction[start..end])
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::test_verify_with_alignment,
        bytemuck::bytes_of,
        solana_secp256r1_program::{
            new_secp256r1_instruction_with_signature, sign_message, DATA_START, SECP256R1_ORDER,
        },
    };

    fn test_case(
        num_signatures: u16,
        offsets: &Secp256r1SignatureOffsets,
    ) -> Result<(), PrecompileError> {
        assert_eq!(
            bytemuck::bytes_of(offsets).len(),
            SIGNATURE_OFFSETS_SERIALIZED_SIZE
        );

        let mut instruction_data = vec![0u8; DATA_START];
        instruction_data[0..SIGNATURE_OFFSETS_START].copy_from_slice(bytes_of(&num_signatures));
        instruction_data[SIGNATURE_OFFSETS_START..DATA_START].copy_from_slice(bytes_of(offsets));
        test_verify_with_alignment(
            verify,
            &instruction_data,
            &[&[0u8; 100]],
            &FeatureSet::all_enabled(),
        )
    }

    #[test]
    fn test_invalid_offsets() {
        solana_logger::setup();

        let mut instruction_data = vec![0u8; DATA_START];
        let offsets = Secp256r1SignatureOffsets::default();
        instruction_data[0..SIGNATURE_OFFSETS_START].copy_from_slice(bytes_of(&1u16));
        instruction_data[SIGNATURE_OFFSETS_START..DATA_START].copy_from_slice(bytes_of(&offsets));
        instruction_data.truncate(instruction_data.len() - 1);

        assert_eq!(
            test_verify_with_alignment(
                verify,
                &instruction_data,
                &[&[0u8; 100]],
                &FeatureSet::all_enabled()
            ),
            Err(PrecompileError::InvalidInstructionDataSize)
        );

        let offsets = Secp256r1SignatureOffsets {
            signature_instruction_index: 1,
            ..Secp256r1SignatureOffsets::default()
        };
        assert_eq!(
            test_case(1, &offsets),
            Err(PrecompileError::InvalidDataOffsets)
        );

        let offsets = Secp256r1SignatureOffsets {
            message_instruction_index: 1,
            ..Secp256r1SignatureOffsets::default()
        };
        assert_eq!(
            test_case(1, &offsets),
            Err(PrecompileError::InvalidDataOffsets)
        );

        let offsets = Secp256r1SignatureOffsets {
            public_key_instruction_index: 1,
            ..Secp256r1SignatureOffsets::default()
        };
        assert_eq!(
            test_case(1, &offsets),
            Err(PrecompileError::InvalidDataOffsets)
        );
    }

    #[test]
    fn test_invalid_signature_data_size() {
        solana_logger::setup();

        // Test data.len() < SIGNATURE_OFFSETS_START
        let small_data = vec![0u8; SIGNATURE_OFFSETS_START - 1];
        assert_eq!(
            test_verify_with_alignment(verify, &small_data, &[&[]], &FeatureSet::all_enabled()),
            Err(PrecompileError::InvalidInstructionDataSize)
        );

        // Test num_signatures == 0
        let mut zero_sigs_data = vec![0u8; DATA_START];
        zero_sigs_data[0] = 0; // Set num_signatures to 0
        assert_eq!(
            test_verify_with_alignment(verify, &zero_sigs_data, &[&[]], &FeatureSet::all_enabled()),
            Err(PrecompileError::InvalidInstructionDataSize)
        );

        // Test num_signatures > 8
        let mut too_many_sigs = vec![0u8; DATA_START];
        too_many_sigs[0] = 9; // Set num_signatures to 9
        assert_eq!(
            test_verify_with_alignment(verify, &too_many_sigs, &[&[]], &FeatureSet::all_enabled()),
            Err(PrecompileError::InvalidInstructionDataSize)
        );
    }
    #[test]
    fn test_message_data_offsets() {
        let offsets = Secp256r1SignatureOffsets {
            message_data_offset: 99,
            message_data_size: 1,
            ..Secp256r1SignatureOffsets::default()
        };
        assert_eq!(
            test_case(1, &offsets),
            Err(PrecompileError::InvalidSignature)
        );

        let offsets = Secp256r1SignatureOffsets {
            message_data_offset: 100,
            message_data_size: 1,
            ..Secp256r1SignatureOffsets::default()
        };
        assert_eq!(
            test_case(1, &offsets),
            Err(PrecompileError::InvalidDataOffsets)
        );

        let offsets = Secp256r1SignatureOffsets {
            message_data_offset: 100,
            message_data_size: 1000,
            ..Secp256r1SignatureOffsets::default()
        };
        assert_eq!(
            test_case(1, &offsets),
            Err(PrecompileError::InvalidDataOffsets)
        );

        let offsets = Secp256r1SignatureOffsets {
            message_data_offset: u16::MAX,
            message_data_size: u16::MAX,
            ..Secp256r1SignatureOffsets::default()
        };
        assert_eq!(
            test_case(1, &offsets),
            Err(PrecompileError::InvalidDataOffsets)
        );
    }

    #[test]
    fn test_pubkey_offset() {
        let offsets = Secp256r1SignatureOffsets {
            public_key_offset: u16::MAX,
            ..Secp256r1SignatureOffsets::default()
        };
        assert_eq!(
            test_case(1, &offsets),
            Err(PrecompileError::InvalidDataOffsets)
        );

        let offsets = Secp256r1SignatureOffsets {
            public_key_offset: 100 - (COMPRESSED_PUBKEY_SERIALIZED_SIZE as u16) + 1,
            ..Secp256r1SignatureOffsets::default()
        };
        assert_eq!(
            test_case(1, &offsets),
            Err(PrecompileError::InvalidDataOffsets)
        );
    }

    #[test]
    fn test_signature_offset() {
        let offsets = Secp256r1SignatureOffsets {
            signature_offset: u16::MAX,
            ..Secp256r1SignatureOffsets::default()
        };
        assert_eq!(
            test_case(1, &offsets),
            Err(PrecompileError::InvalidDataOffsets)
        );

        let offsets = Secp256r1SignatureOffsets {
            signature_offset: 100 - (SIGNATURE_SERIALIZED_SIZE as u16) + 1,
            ..Secp256r1SignatureOffsets::default()
        };
        assert_eq!(
            test_case(1, &offsets),
            Err(PrecompileError::InvalidDataOffsets)
        );
    }
}
