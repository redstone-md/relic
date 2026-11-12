import { Toaster as Sonner, type ToasterProps } from "sonner"

import { Icon } from "@/components/icon"
import { useTheme } from "@/lib/theme"

const Toaster = ({ ...props }: ToasterProps) => {
  const { resolved } = useTheme()

  return (
    <Sonner
      theme={resolved}
      className="toaster group"
      icons={{
        success: <Icon name="circle-check" appear />,
        info: <Icon name="bell" appear />,
        warning: <Icon name="alert" appear />,
        error: <Icon name="alert" appear />,
        loading: <Icon name="spinner" active />,
      }}
      style={
        {
          "--normal-bg": "var(--popover)",
          "--normal-text": "var(--popover-foreground)",
          "--normal-border": "var(--border)",
          "--border-radius": "var(--radius)",
        } as React.CSSProperties
      }
      {...props}
    />
  )
}

export { Toaster }
