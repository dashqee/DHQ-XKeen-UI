import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogMedia,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { IconDownload } from '@tabler/icons-react'
import { useEffect, useState } from 'react'
import { useAppContext } from '../../lib/store'

/** Snoozing lives in sessionStorage on purpose: "напомнить в следующий раз"
 *  means the next time the panel is opened, so the answer must not outlive the
 *  tab. Keyed by version, so a newer release asks again in the same session. */
const SNOOZE_KEY = 'dhq-update-snoozed'

function snoozedFor(version: string): boolean {
  try {
    return !!version && sessionStorage.getItem(SNOOZE_KEY) === version
  } catch {
    // Private mode and blocked site data both throw here. Asking again is the
    // better failure: the prompt is dismissible, a swallowed update is not.
    return false
  }
}

function snooze(version: string) {
  try {
    sessionStorage.setItem(SNOOZE_KEY, version)
  } catch {
    /* nothing to remember it with; the prompt simply reappears */
  }
}

/** Offers the new panel release once per visit, rather than leaving it to a
 *  toast the user scrolls past. */
export function UpdatePrompt() {
  const { state, dispatch } = useAppContext()
  const { isOutdatedUI, latestUI, version } = state
  const [dismissed, setDismissed] = useState(false)

  // A version that arrives after the first render (the check is async) must
  // still be offered, and one already snoozed must not reopen.
  useEffect(() => {
    if (isOutdatedUI && latestUI && snoozedFor(latestUI)) setDismissed(true)
  }, [isOutdatedUI, latestUI])

  const open = isOutdatedUI && !dismissed

  const install = () => {
    setDismissed(true)
    dispatch({ type: 'SET_UPDATE_MODAL_CORE', core: 'self' })
    dispatch({ type: 'SHOW_MODAL', modal: 'showUpdateModal', show: true })
  }

  const later = () => {
    setDismissed(true)
    if (latestUI) snooze(latestUI)
  }

  return (
    <AlertDialog open={open} onOpenChange={(next) => !next && later()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogMedia>
            <IconDownload />
          </AlertDialogMedia>
          <AlertDialogTitle>Доступно обновление</AlertDialogTitle>
          <AlertDialogDescription>
            {latestUI ? (
              <>
                Вышла версия <strong>{latestUI}</strong>
                {version ? <> — у вас {version}</> : null}. Установить сейчас? Панель перезапустится сама.
              </>
            ) : (
              <>Доступна новая версия DHQClash Router. Установить сейчас? Панель перезапустится сама.</>
            )}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel onClick={later}>Напомнить позже</AlertDialogCancel>
          <AlertDialogAction onClick={install}>Обновить</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
