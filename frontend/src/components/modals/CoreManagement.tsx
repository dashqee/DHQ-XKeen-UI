import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { IconCpu } from '@tabler/icons-react'
import { useAppContext, useModalContext } from '../../lib/store'

interface Props {
  onOpenUpdate: (core: string) => void
}

export function CoreManageModal({ onOpenUpdate }: Props) {
  const { state } = useAppContext()
  const { modals, dispatch } = useModalContext()
  const { coreVersions, availableCores } = state
  const isInstalled = availableCores.includes('mihomo')

  const close = () => dispatch({ type: 'SHOW_MODAL', modal: 'showCoreManageModal', show: false })

  return (
    <Dialog open={modals.showCoreManageModal} onOpenChange={(open) => !open && close()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2 pb-3">
            <IconCpu size={24} className="text-chart-2" /> Управление ядром
          </DialogTitle>
        </DialogHeader>

        <div className="flex items-center justify-between gap-4">
          <div>
            <div className="flex items-center gap-3">
              <span className="text-sm font-medium">Mihomo</span>
              {isInstalled ? (
                <Badge variant="outline" className="rounded-sm border-none bg-green-500/10 px-2 text-xs text-green-400">
                  Активно
                </Badge>
              ) : (
                <Badge variant="outline" className="rounded-sm border-none bg-red-500/10 px-2 text-xs text-red-400">
                  Не установлено
                </Badge>
              )}
            </div>
            {isInstalled && <p className="text-muted-foreground mt-0.5 text-xs">{coreVersions.mihomo || 'Установлено'}</p>}
          </div>
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              close()
              onOpenUpdate('mihomo')
            }}
          >
            {isInstalled ? 'Обновить' : 'Установить'}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}
