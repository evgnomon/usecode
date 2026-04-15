packer {
  required_plugins {
    qemu = {
      version = ">= 1.1.0"
      source  = "github.com/hashicorp/qemu"
    }
    ansible = {
      version = ">= 1.1.0"
      source  = "github.com/hashicorp/ansible"
    }
  }
}

variable "user" {
  type    = string
  default = env("USER")
}

variable "user_home" {
  type    = string
  default = env("HOME")
}

variable "vm_name" {
  type    = string
  default = "zamin"
}

variable "cpu_cores" {
  type    = number
  default = 2
}

variable "memory" {
  type    = number
  default = 1024
}

variable "disk_size" {
  type    = string
  default = "5G"   # ← Your 5 GB disk
}

source "qemu" "zamin" {
  vm_name          = var.vm_name
  iso_url          = "${var.user_home}/.cache/blueprint/debian/linux_amd64/debian_13/debian_13.bin"
  iso_checksum     = "none"
  disk_image       = true
  disk_size        = var.disk_size
  format           = "qcow2"
  accelerator      = "kvm"
  use_backing_file = false
  memory           = var.memory
  cpus             = var.cpu_cores

  ssh_username     = "packer"
  ssh_password     = "packer"
  ssh_timeout      = "5m"

  shutdown_command = "echo 'packer' | sudo -S sh -c 'userdel -rf packer; shutdown -P now'"

  http_directory   = "http"  # ← Enable this

  qemuargs = [
    ["-cpu", "host"],
    ["-m", "${var.memory}M"],
    ["-smbios", "type=1,serial=ds=nocloud-net;s=http://{{ .HTTPIP }}:{{ .HTTPPort }}/"]  # ← Enable this
  ]

  boot_wait    = "30s"  # Give cloud-init time to run
  boot_command = []
}

build {
  sources = ["source.qemu.zamin"]

  provisioner "ansible" {
    playbook_file   = "./main.yaml"
    user            = "packer"
    extra_arguments = ["--extra-vars", "ansible_python_interpreter=/usr/bin/python3"]
  }

  # If you want a final clean image (shut down + export)
  post-processor "shell-local" {
    inline = ["echo VM ${var.vm_name} with 40GB disk is ready at ./output-qemu/${var.vm_name}"]
  }
}
