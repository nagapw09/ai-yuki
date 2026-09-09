import { useT } from '../i18n'

/**
 * Заглушка нереализованного раздела.
 *
 * Показывает прямо, что раздела ещё нет, вместо пустого экрана или ложного
 * интерфейса, за которым ничего не стоит. Это та же логика, что и в ТЗ §5:
 * не выдавать за сделанное то, что не сделано.
 */
export function Placeholder({ title }: { title: string }) {
  const t = useT()

  return (
    <div
      style={{
        display: 'grid',
        placeContent: 'center',
        gap: 'var(--space-3)',
        height: '100%',
        padding: 'var(--space-6)',
        textAlign: 'center',
      }}
    >
      <h1
        style={{
          margin: 0,
          fontSize: 'var(--text-lg)',
          fontWeight: 'var(--weight-medium)',
          letterSpacing: 'var(--tracking-tight)',
        }}
      >
        {title}
      </h1>
      <p style={{ margin: 0, color: 'var(--text-muted)', fontSize: 'var(--text-sm)' }}>
        {t('screen.soon.body')}
      </p>
    </div>
  )
}
