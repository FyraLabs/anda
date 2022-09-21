%undefine _disable_source_fetch

Name:           umpkg
Version:        0.3.63
Release:        2%{?dist}
Summary:        The Ultramarine Packager tool
URL:            https://ultramarine-linux.org
Source0:        https://github.com/Ultramarine-Linux/umpkg/archive/refs/tags/%{version}.tar.gz
License:        MIT
BuildRequires:  python3-devel
Requires:       mock
Requires:       python3-arrow
Group:          Applications/Internet
BuildArch:      noarch
%description
umpkg is an RPM packaging tool for Ultramarine Linux. It can be used to quickly create RPMs from source code, and pushing them to a repository.
Instead of writing long and complex commandline arguments for RPMBuild and Mock, umpkg uses a configuration file to specify the build process for a reproducible build.


%prep
%autosetup -n umpkg-%{version}

%generate_buildrequires
%pyproject_buildrequires


%build
%pyproject_wheel


%install
%pyproject_install
%pyproject_save_files umpkg

%files -f %{pyproject_files}
%{_bindir}/umpkg

%changelog
* Mon May 30 2022 Cappy Ishihara <cappy@cappuchino.xyz> - 0.3.3-2.um36
- Updated Packaging

* Sat May 28 2022 Cappy Ishihara <cappy@cappuchino.xyz> - 0.3.1-1.um36
- Initial Rewrite
