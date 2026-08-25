import { useEffect, useRef, type ReactNode } from 'react'
import { createPortal } from 'react-dom'

export type ModalProps = {
  isOpen: boolean
  onClose: () => void
  title?: string
  children: ReactNode
  panelClassName?: string
  dismissible?: boolean
  zIndexClassName?: string
  rootTestId?: string
}

export function Modal({
  isOpen,
  onClose,
  title,
  children,
  dismissible = true,
  panelClassName,
  zIndexClassName = 'z-[9999]',
  rootTestId,
}: ModalProps) {
  const modalRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!isOpen || !dismissible) return
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose()
    }
    document.addEventListener('keydown', handler)
    return () => document.removeEventListener('keydown', handler)
  }, [isOpen, onClose, dismissible])

  useEffect(() => {
    if (isOpen) modalRef.current?.focus()
  }, [isOpen])

  if (!isOpen) return null

  return createPortal(
    <div
      className={`app-modal-portal-root fixed inset-0 ${zIndexClassName} flex items-center justify-center p-4`}
      data-testid={rootTestId}
    >
      <div
        className="app-modal-backdrop"
        onClick={dismissible ? onClose : undefined}
        role="presentation"
        aria-hidden="true"
      />
      <div
        ref={modalRef}
        tabIndex={-1}
        className={`app-modal-panel${panelClassName ? ` ${panelClassName}` : ''}`}
        role="dialog"
        aria-modal="true"
        aria-labelledby={title ? 'modal-title' : undefined}
      >
        {title && (
          <div className={`app-modal-header${dismissible ? '' : ' app-modal-header--blocking'}`}>
            <h2 id="modal-title" className="text-lg font-semibold" style={{ color: 'var(--ink)' }}>
              {title}
            </h2>
            {dismissible ? (
              <button type="button" onClick={onClose} className="btn-muted !min-h-0 !px-2.5 !py-2" aria-label="Close modal">
                ×
              </button>
            ) : null}
          </div>
        )}
        {children}
      </div>
    </div>,
    document.body
  )
}
