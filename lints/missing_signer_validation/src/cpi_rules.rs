use anchor_lints_utils::cpi_types::CpiKind;

pub struct CpiMeta {
    pub cpi_kind: CpiKind,
    pub signer_source: SignerSource,
    pub signer_field_name: &'static str,
}

pub enum SignerSource {
    ContextSigner,   // signer inside CPI accounts struct
    ArgIndex(usize), // signer is directly passed at this arg index
}

static CPI_RULES: &[CpiMeta] = &[
    CpiMeta {
        cpi_kind: CpiKind::SystemTransfer,
        signer_source: SignerSource::ContextSigner,
        signer_field_name: "from",
    },
    CpiMeta {
        cpi_kind: CpiKind::Transfer,
        signer_source: SignerSource::ContextSigner,
        signer_field_name: "authority",
    },
    CpiMeta {
        cpi_kind: CpiKind::MintTo,
        signer_source: SignerSource::ContextSigner,
        signer_field_name: "authority",
    },
    CpiMeta {
        cpi_kind: CpiKind::Burn,
        signer_source: SignerSource::ContextSigner,
        signer_field_name: "authority",
    },
    CpiMeta {
        cpi_kind: CpiKind::Token2022Transfer,
        signer_source: SignerSource::ArgIndex(3),
        signer_field_name: "authority",
    },
    CpiMeta {
        cpi_kind: CpiKind::Token2022TransferChecked,
        signer_source: SignerSource::ArgIndex(4),
        signer_field_name: "authority",
    },
    CpiMeta {
        cpi_kind: CpiKind::CreateAta,
        signer_source: SignerSource::ContextSigner,
        signer_field_name: "authority",
    },
    CpiMeta {
        cpi_kind: CpiKind::SetAuthority,
        signer_source: SignerSource::ContextSigner,
        signer_field_name: "current_authority",
    },
    CpiMeta {
        cpi_kind: CpiKind::CloseAccount,
        signer_source: SignerSource::ContextSigner,
        signer_field_name: "authority",
    },
    CpiMeta {
        cpi_kind: CpiKind::FreezeAccount,
        signer_source: SignerSource::ContextSigner,
        signer_field_name: "authority",
    },
    CpiMeta {
        cpi_kind: CpiKind::ThawAccount,
        signer_source: SignerSource::ContextSigner,
        signer_field_name: "authority",
    },
    CpiMeta {
        cpi_kind: CpiKind::Approve,
        signer_source: SignerSource::ContextSigner,
        signer_field_name: "authority",
    },
    CpiMeta {
        cpi_kind: CpiKind::Revoke,
        signer_source: SignerSource::ContextSigner,
        signer_field_name: "authority",
    },
    CpiMeta {
        cpi_kind: CpiKind::Token2022MintToChecked,
        signer_source: SignerSource::ArgIndex(3),
        signer_field_name: "mint_authority",
    },
    CpiMeta {
        cpi_kind: CpiKind::Token2022BurnChecked,
        signer_source: SignerSource::ArgIndex(3),
        signer_field_name: "authority",
    },
    CpiMeta {
        cpi_kind: CpiKind::SyncNative,
        signer_source: SignerSource::ContextSigner,
        signer_field_name: "account",
    },
];

pub fn get_cpi_rule(cpi_kind: CpiKind) -> Option<&'static CpiMeta> {
    CPI_RULES.iter().find(|r| r.cpi_kind == cpi_kind)
}
