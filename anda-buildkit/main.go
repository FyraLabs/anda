package main

import (
	"bytes"
	"context"
	"net/http"

	"github.com/ethereum/go-ethereum/rpc"
	"github.com/moby/buildkit/client/llb"
)

type BuilderService struct{}

type JobSpec struct {
	ID      string `json:"id"`
	Repo    string `json:"repo"`
	Ref     string `json:"ref"`
	Builder string `json:"builder"`
}

func (b *BuilderService) JobLLB(spec JobSpec) (string, error) {
	state := llb.Image(spec.Builder).Run(llb.Shlex("echo hello!")).
		AddMount("/src", llb.Git(spec.Repo, spec.Ref)).
		Dir("/src").
		Run(llb.Shlex("dnf install -y rpmdevtools rpmbuild dnf-plugins-core")).
		Run(llb.Shlex("dnf builddep -y package.spec")).
		Run(llb.Shlex("rpmdev-setuptree")).
		Run(llb.Shlex("rpmbuild -ba package.spec -define \"_rpmdir $(pwd)\" --define \"_srcrpmdir $(pwd)\" --undefine=_disable_source_fetch --define '_sourcedir .'"))

	bc, err := state.Root().Marshal(context.TODO(), llb.LinuxAmd64)
	if err != nil {
		return "", err
	}

	var buf bytes.Buffer

	llb.WriteTo(bc, &buf)

	return buf.String(), nil
}

func main() {
	builder := new(BuilderService)
	server := rpc.NewServer()
	server.RegisterName("builder", builder)

	if err := http.ListenAndServe(":8080", server); err != nil {
		panic(err)
	}
}
