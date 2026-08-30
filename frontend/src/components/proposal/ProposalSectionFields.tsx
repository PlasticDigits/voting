import {
  SECTION_DISPLAY_ORDER,
  SECTION_HELP,
  SECTION_LABELS,
  MAX_SUMMARY_VISIBLE_CHARS,
  MIN_SECTION_VISIBLE_CHARS,
  type ProposalSections,
  type SectionKey,
  type SectionValidation,
  plainToSectionHtml,
  visibleLen,
} from '@/utils/proposalSections'

type Props = {
  values: Record<SectionKey, string>
  errors: SectionValidation['errors']
  disabled?: boolean
  onChange: (key: SectionKey, value: string) => void
}

export default function ProposalSectionFields({ values, errors, disabled, onChange }: Props) {
  return (
    <div className="proposal-sections-form">
      {SECTION_DISPLAY_ORDER.map((key) => {
        const html = plainToSectionHtml(values[key])
        const count = visibleLen(html)
        const required = key !== 'context'
        const max = key === 'summary' ? MAX_SUMMARY_VISIBLE_CHARS : undefined
        return (
          <label className="field" key={key}>
            <span>
              {SECTION_LABELS[key]}
              {required ? '' : ' (optional)'}
            </span>
            <p className="field-help">{SECTION_HELP[key]}</p>
            <textarea
              value={values[key]}
              onChange={(e) => onChange(key, e.target.value)}
              disabled={disabled}
              rows={key === 'summary' ? 3 : 5}
              maxLength={key === 'summary' ? MAX_SUMMARY_VISIBLE_CHARS + 80 : undefined}
              data-testid={`proposal-section-${key}`}
            />
            <span className="field-count" data-testid={`section-count-${key}`}>
              {count}
              {required ? ` / ${MIN_SECTION_VISIBLE_CHARS} min` : ''}
              {max != null ? ` · ${max} max` : ''}
            </span>
            {errors[key] && (
              <span className="field-error" role="status">
                {errors[key]}
              </span>
            )}
          </label>
        )
      })}
    </div>
  )
}

export function ProposalSectionsPreview({ sections }: { sections: ProposalSections }) {
  return (
    <div className="proposal-preview" data-testid="proposal-preview">
      <h2>Voter preview</h2>
      <ProposalSectionView sections={sections} />
    </div>
  )
}

export function ProposalSectionView({
  sections,
  fallbackHtml,
}: {
  sections?: ProposalSections | Record<string, string> | null
  fallbackHtml?: string
}) {
  if (sections) {
    return (
      <div className="proposal-body" data-testid="proposal-sections">
        {SECTION_DISPLAY_ORDER.map((key) => {
          const html = sections[key]
          if (!html) return null
          return (
            <section key={key} data-testid={`proposal-section-view-${key}`}>
              <h2>{SECTION_LABELS[key as SectionKey] ?? key}</h2>
              <div dangerouslySetInnerHTML={{ __html: html }} />
            </section>
          )
        })}
      </div>
    )
  }
  if (fallbackHtml) {
    return (
      <article
        className="proposal-body"
        data-testid="proposal-legacy-body"
        dangerouslySetInnerHTML={{ __html: fallbackHtml }}
      />
    )
  }
  return null
}
